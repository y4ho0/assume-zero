use crate::model::{EvidenceLevel, Report, ScenarioStatus};
use crate::platform;
use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

const MAX_SAVED_REPORT_BYTES: u64 = 64 * 1_048_576;

pub fn persist(project: &Path, report: &Report, formats: &[String]) -> Result<PathBuf> {
    validate_run_id(&report.run_id)?;
    let json = serde_json::to_vec_pretty(report)?;
    if json.len() as u64 > MAX_SAVED_REPORT_BYTES {
        bail!(
            "generated report exceeds the {} MiB persisted-report limit; reduce run or log budgets",
            MAX_SAVED_REPORT_BYTES / 1_048_576
        );
    }
    let runs = report_root(project, true)?;
    let directory = runs.join(&report.run_id);
    if fs::symlink_metadata(&directory).is_ok() {
        bail!("report run `{}` already exists", report.run_id);
    }

    let staging = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(&runs)
        .context("could not create a staged report directory")?;
    let staging_root = staging
        .path()
        .canonicalize()
        .context("staged report directory could not be resolved")?;
    ensure_contained(&runs, &staging_root, "staged report directory")?;
    write_new_file(&staging_root.join("report.json"), &json)?;
    if formats.iter().any(|format| format == "markdown") {
        write_new_file(&staging_root.join("report.md"), markdown(report).as_bytes())?;
    }
    if formats.iter().any(|format| format == "junit") {
        write_new_file(&staging_root.join("report.junit.xml"), &junit(report)?)?;
    }
    fs::rename(staging.path(), &directory).with_context(|| {
        format!(
            "could not publish report run `{}` inside the project",
            report.run_id
        )
    })?;
    checked_directory(&runs, &directory, "report run directory")
}

pub fn load(project: &Path, run_id: &str) -> Result<Report> {
    let directory = existing_run_directory(project, run_id)?;
    let path = checked_regular_file(&directory, &directory.join("report.json"), "saved report")?;
    let mut file = File::open(&path)
        .with_context(|| format!("run `{run_id}` was not found at `{}`", path.display()))?;
    if !file.metadata()?.is_file() {
        bail!("saved report for run `{run_id}` is not a regular file");
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_SAVED_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SAVED_REPORT_BYTES {
        bail!(
            "saved report for run `{run_id}` exceeds the {} MiB read limit",
            MAX_SAVED_REPORT_BYTES / 1_048_576
        );
    }
    let report: Report =
        serde_json::from_slice(&bytes).context("saved report is not valid report schema v1")?;
    validate_run_id(&report.run_id).context("saved report contains an unsafe run ID")?;
    if report.run_id != run_id {
        bail!(
            "saved report run ID `{}` does not match requested run `{run_id}`",
            report.run_id
        );
    }
    Ok(report)
}

pub fn print_terminal(report: &Report, quiet: bool) {
    if quiet {
        return;
    }
    println!("AssumeZero completed.\n");
    println!("Command:\n  {}\n", display_command(&report.command));
    let accepted = report.baseline.iter().filter(|run| run.accepted).count();
    println!(
        "Baseline:\n  {} — {accepted}/{} accepted\n",
        report.baseline_status,
        report.baseline.len()
    );
    let passed = report
        .scenarios
        .iter()
        .filter(|scenario| scenario.status == ScenarioStatus::Pass)
        .count();
    let failed = report
        .scenarios
        .iter()
        .filter(|scenario| scenario.status == ScenarioStatus::Fail)
        .count();
    let skipped = report
        .scenarios
        .iter()
        .filter(|scenario| scenario.status == ScenarioStatus::SkippedUnsupported)
        .count();
    let inconclusive = report
        .scenarios
        .iter()
        .filter(|scenario| {
            matches!(
                scenario.status,
                ScenarioStatus::Inconclusive | ScenarioStatus::InfrastructureError
            )
        })
        .count();
    println!(
        "Scenarios:\n  {passed} passed\n  {failed} failed\n  {skipped} skipped\n  {inconclusive} inconclusive/infrastructure\n"
    );
    for finding in &report.findings {
        println!(
            "{}\n  Scenario: {}\n  Evidence: {}\n\n  {}\n  {}\n\n  Conclusion:\n  {}\n\n  Next:\n  {}\n\n  This does not prove:\n  {}\n",
            finding.id,
            finding.scenario_id,
            evidence_label(finding.evidence),
            finding.changed,
            finding.observed,
            finding.conclusion,
            finding.next_step,
            finding.not_proven
        );
    }
    println!(
        "Secret values persisted: no recognized environment or CLI secret values are report fields\nSource workspace unchanged: {}\nRun ID: {}\n",
        if report.workspace_integrity.source_unchanged {
            "yes"
        } else {
            "NO"
        },
        report.run_id
    );
}

pub fn markdown(report: &Report) -> String {
    let mut output = format!(
        "# AssumeZero report `{}`\n\n\
         - Command: `{}`\n\
         - Baseline: **{}**\n\
         - Platform: `{}` / `{}`\n\
         - Source workspace unchanged: **{}**\n\
         - Environment variable values persisted: **no**\n\n\
         - Recognized CLI secret values persisted: **no**\n\n\
         ## Scenarios\n\n\
         | ID | Scenario | Status | Runs | Note |\n\
         |---|---|---:|---:|---|\n",
        report.run_id,
        escape_markdown(&display_command(&report.command)),
        report.baseline_status,
        report.platform.get("os").map_or("unknown", String::as_str),
        report
            .platform
            .get("arch")
            .map_or("unknown", String::as_str),
        report.workspace_integrity.source_unchanged,
    );
    for scenario in &report.scenarios {
        output.push_str(&format!(
            "| {} | {} | {:?} | {} | {} |\n",
            scenario.id,
            scenario.name,
            scenario.status,
            scenario.runs.len(),
            escape_markdown(&scenario.note)
        ));
    }
    output.push_str("\n## Findings\n\n");
    if report.findings.is_empty() {
        output.push_str("No confirmed or suspected hidden assumptions were found in the executed scenario set.\n");
    }
    for finding in &report.findings {
        output.push_str(&format!(
            "### {} — {} ({})\n\n\
             **Changed:** {}\n\n\
             **Observed:** {}\n\n\
             **Conclusion:** {}\n\n\
             **Next step:** {}\n\n\
             **Not proven:** {}\n\n",
            finding.id,
            finding.scenario_id,
            evidence_label(finding.evidence),
            finding.changed,
            finding.observed,
            finding.conclusion,
            finding.next_step,
            finding.not_proven
        ));
    }
    output.push_str("## Safety note\n\nAssumeZero ran the command in copied workspaces. This protects source files from direct command writes; it does not sandbox untrusted code or prevent network and other machine access.\n");
    output
}

pub fn junit(report: &Report) -> Result<Vec<u8>> {
    let failures = report
        .scenarios
        .iter()
        .filter(|item| item.status == ScenarioStatus::Fail)
        .count();
    let skipped = report
        .scenarios
        .iter()
        .filter(|item| item.status == ScenarioStatus::SkippedUnsupported)
        .count();
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <testsuite name=\"AssumeZero\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\">",
        report.scenarios.len()
    );
    for scenario in &report.scenarios {
        xml.push_str("<testcase classname=\"assumezero.scenario\" name=\"");
        xml.push_str(&xml_escape(&scenario.name));
        xml.push_str("\">");
        match scenario.status {
            ScenarioStatus::Fail => {
                xml.push_str("<failure message=\"");
                xml.push_str(&xml_escape(&scenario.note));
                xml.push_str("\">");
                xml.push_str(&xml_escape(&scenario.description));
                xml.push_str("</failure>");
            }
            ScenarioStatus::SkippedUnsupported => {
                xml.push_str("<skipped message=\"");
                xml.push_str(&xml_escape(&scenario.note));
                xml.push_str("\"/>");
            }
            ScenarioStatus::InfrastructureError | ScenarioStatus::Inconclusive => {
                xml.push_str("<error message=\"");
                xml.push_str(&xml_escape(&scenario.note));
                xml.push_str("\"/>");
            }
            ScenarioStatus::Pass => {}
        }
        xml.push_str("</testcase>");
    }
    xml.push_str("</testsuite>");
    Ok(xml.into_bytes())
}

pub fn write_requested_format(project: &Path, report: &Report, format: &str) -> Result<PathBuf> {
    validate_run_id(&report.run_id)?;
    let directory = existing_run_directory(project, &report.run_id)?;
    let (path, bytes) = match format {
        "json" => (
            directory.join("report.json"),
            serde_json::to_vec_pretty(report)?,
        ),
        "markdown" => (directory.join("report.md"), markdown(report).into_bytes()),
        "junit" => (directory.join("report.junit.xml"), junit(report)?),
        other => anyhow::bail!("unsupported report format `{other}`"),
    };
    if format == "json" && bytes.len() as u64 > MAX_SAVED_REPORT_BYTES {
        bail!(
            "generated report exceeds the {} MiB persisted-report limit; reduce run or log budgets",
            MAX_SAVED_REPORT_BYTES / 1_048_576
        );
    }
    atomic_write_regular(&directory, &path, &bytes)?;
    Ok(path)
}

fn validate_run_id(run_id: &str) -> Result<()> {
    let valid_ulid = run_id
        .parse::<ulid::Ulid>()
        .is_ok_and(|parsed| parsed.to_string() == run_id);
    if !valid_ulid || !platform::is_single_normal_component(run_id) {
        bail!(
            "run ID `{run_id}` must be a canonical 26-character ULID and one normal path component"
        );
    }
    Ok(())
}

fn report_root(project: &Path, create: bool) -> Result<PathBuf> {
    let project_root = project
        .canonicalize()
        .context("project root could not be resolved for report access")?;
    let metadata_root = project_root.join(".assumezero");
    let metadata_root = ensure_plain_directory(
        &project_root,
        &metadata_root,
        create,
        "AssumeZero metadata directory",
    )?;
    let runs = metadata_root.join("runs");
    ensure_plain_directory(&project_root, &runs, create, "report root")
}

fn existing_run_directory(project: &Path, run_id: &str) -> Result<PathBuf> {
    validate_run_id(run_id)?;
    let runs = report_root(project, false)?;
    let directory = runs.join(run_id);
    checked_directory(&runs, &directory, "report run directory")
        .with_context(|| format!("run `{run_id}` was not found"))
}

fn ensure_plain_directory(
    trusted_root: &Path,
    path: &Path,
    create: bool,
    label: &str,
) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound && create => {
            fs::create_dir(path)
                .with_context(|| format!("could not create {label} `{}`", path.display()))?;
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("{label} `{}` is unavailable", path.display()));
        }
    }
    checked_directory(trusted_root, path, label)
}

fn checked_directory(trusted_root: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} `{}` could not be inspected", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "{label} `{}` must be a real directory, not a symlink or special file",
            path.display()
        );
    }
    let resolved = path
        .canonicalize()
        .with_context(|| format!("{label} `{}` could not be resolved", path.display()))?;
    ensure_contained(trusted_root, &resolved, label)?;
    Ok(resolved)
}

fn checked_regular_file(trusted_root: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} `{}` could not be inspected", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "{label} `{}` must be a regular file, not a symlink or special file",
            path.display()
        );
    }
    let resolved = path
        .canonicalize()
        .with_context(|| format!("{label} `{}` could not be resolved", path.display()))?;
    ensure_contained(trusted_root, &resolved, label)?;
    Ok(resolved)
}

fn ensure_contained(root: &Path, candidate: &Path, label: &str) -> Result<()> {
    if candidate.strip_prefix(root).is_err() {
        bail!(
            "{label} `{}` resolves outside trusted root `{}`",
            candidate.display(),
            root.display()
        );
    }
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("could not create report file `{}`", path.display()))?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

fn atomic_write_regular(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            bail!(
                "report output `{}` must be absent or a regular file, not a symlink or special file",
                path.display()
            );
        }
        Ok(_) => {
            checked_regular_file(directory, path, "report output")?;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("report output `{}` could not be inspected", path.display())
            });
        }
    }

    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .context("could not create a staged report file")?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "could not atomically write report output `{}`",
                path.display()
            )
        })?;
    checked_regular_file(directory, path, "report output")?;
    Ok(())
}

pub fn explain(report: &Report) -> String {
    if report.findings.is_empty() {
        return format!(
            "Run {} found no evidence-backed hidden assumption in the scenarios that actually executed. This does not prove the project is environment-independent.",
            report.run_id
        );
    }
    let mut output = String::new();
    for finding in &report.findings {
        output.push_str(&format!(
            "{} ({})\nChanged: {}\nObserved: {}\nConclusion: {}\nNext: {}\nNot proven: {}\n\n",
            finding.scenario_id,
            evidence_label(finding.evidence),
            finding.changed,
            finding.observed,
            finding.conclusion,
            finding.next_step,
            finding.not_proven
        ));
    }
    output
}

pub fn exit_code(report: &Report, suspected_is_failure: bool) -> u8 {
    match report.baseline_status.as_str() {
        "BASELINE_FAILED" | "BASELINE_UNSTABLE" => return 2,
        _ => {}
    }
    if report.baseline.iter().any(|run| run.interrupted)
        || report
            .scenarios
            .iter()
            .flat_map(|scenario| &scenario.runs)
            .any(|run| run.interrupted)
    {
        return 5;
    }
    let failing = report.findings.iter().any(|finding| {
        matches!(
            finding.evidence,
            EvidenceLevel::Proven | EvidenceLevel::Confirmed
        ) || (suspected_is_failure && finding.evidence == EvidenceLevel::Suspected)
    });
    u8::from(failing)
}

fn display_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.chars().any(char::is_whitespace) {
                format!("{part:?}")
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_markdown(input: &str) -> String {
    input.replace('|', "\\|").replace('\n', " ")
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const fn evidence_label(level: EvidenceLevel) -> &'static str {
    match level {
        EvidenceLevel::Proven => "PROVEN",
        EvidenceLevel::Confirmed => "CONFIRMED",
        EvidenceLevel::Suspected => "SUSPECTED",
        EvidenceLevel::Inconclusive => "INCONCLUSIVE",
        EvidenceLevel::Skipped => "SKIPPED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BudgetEvidence, IntegrityEvidence, ReportConfiguration, ScenarioEvidence};
    use std::collections::BTreeMap;

    fn sample_report(status: ScenarioStatus) -> Report {
        Report {
            schema_version: 1,
            tool_version: "0.1.0".into(),
            run_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            started_at: "0".into(),
            finished_at: "1".into(),
            platform: BTreeMap::new(),
            repository_fingerprint: "abc".into(),
            configuration: ReportConfiguration {
                source: "defaults".into(),
                profile: "quick".into(),
                timeout_seconds: 1,
                baseline_runs: 2,
                confirm_failures: 2,
                workspace_mode: "working-tree".into(),
                report_formats: vec!["json".into()],
            },
            command: vec!["demo".into()],
            baseline: vec![],
            baseline_status: "STABLE".into(),
            scenarios: vec![ScenarioEvidence {
                id: "AZ-S001".into(),
                name: "EMPTY_HOME".into(),
                description: "demo".into(),
                status,
                best_effort: false,
                runs: vec![],
                restored_names: vec![],
                minimization_complete: false,
                note: "note".into(),
            }],
            findings: vec![],
            budget: BudgetEvidence {
                max_total_runs: 1,
                max_total_seconds: 1,
                runs_used: 1,
                elapsed_seconds: 1,
                exhausted: false,
            },
            redaction_summary: BTreeMap::new(),
            workspace_integrity: IntegrityEvidence {
                before_fingerprint: "a".into(),
                after_fingerprint: "a".into(),
                source_unchanged: true,
                git_status_before: None,
                git_status_after: None,
                note: String::new(),
            },
        }
    }

    #[test]
    fn junit_maps_failure_to_failure_element() {
        let xml = String::from_utf8(junit(&sample_report(ScenarioStatus::Fail)).expect("junit"))
            .expect("UTF-8");
        assert!(xml.contains("<failure"));
    }

    #[test]
    fn unsupported_scenario_does_not_fail_exit() {
        assert_eq!(
            exit_code(&sample_report(ScenarioStatus::SkippedUnsupported), false),
            0
        );
    }

    #[test]
    fn junit_escapes_xml_metacharacters() {
        let mut report = sample_report(ScenarioStatus::Fail);
        report.scenarios[0].note = "a & \"b\" < c".into();
        let xml = String::from_utf8(junit(&report).expect("junit")).expect("UTF-8");
        assert!(xml.contains("a &amp; &quot;b&quot; &lt; c"));
    }

    #[test]
    fn run_ids_must_be_one_portable_normal_component() {
        let project = tempfile::tempdir().expect("project");
        for invalid in [
            "",
            ".",
            "..",
            "/absolute",
            "nested/path",
            "nested\\path",
            "C:relative",
            "C:\\absolute",
            "\\\\server\\share",
            "//server/share",
        ] {
            assert!(load(project.path(), invalid).is_err(), "{invalid}");
            let mut report = sample_report(ScenarioStatus::Pass);
            report.run_id = invalid.into();
            assert!(persist(project.path(), &report, &["json".into()]).is_err());
        }
    }

    #[test]
    fn loaded_report_id_must_match_requested_directory() {
        let project = tempfile::tempdir().expect("project");
        let report = sample_report(ScenarioStatus::Pass);
        let directory = persist(project.path(), &report, &["json".into()]).expect("persist");
        let mut changed = report;
        changed.run_id = "other".into();
        fs::write(
            directory.join("report.json"),
            serde_json::to_vec_pretty(&changed).expect("json"),
        )
        .expect("replace fixture");
        assert!(load(project.path(), "01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn report_roots_and_outputs_refuse_symlink_escape() {
        let outside = tempfile::tempdir().expect("outside");
        let outside_file = outside.path().join("outside.md");
        fs::write(&outside_file, "unchanged").expect("outside file");

        let metadata_link_project = tempfile::tempdir().expect("metadata link project");
        std::os::unix::fs::symlink(
            outside.path(),
            metadata_link_project.path().join(".assumezero"),
        )
        .expect("metadata symlink");
        assert!(persist(
            metadata_link_project.path(),
            &sample_report(ScenarioStatus::Pass),
            &["json".into()]
        )
        .is_err());

        let run_link_project = tempfile::tempdir().expect("run link project");
        fs::create_dir_all(run_link_project.path().join(".assumezero/runs")).expect("runs");
        std::os::unix::fs::symlink(
            outside.path(),
            run_link_project
                .path()
                .join(".assumezero/runs/01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        )
        .expect("run symlink");
        assert!(load(run_link_project.path(), "01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());

        let output_link_project = tempfile::tempdir().expect("output link project");
        let report = sample_report(ScenarioStatus::Pass);
        let directory = persist(output_link_project.path(), &report, &["json".into()])
            .expect("persist safe report");
        std::os::unix::fs::symlink(&outside_file, directory.join("report.md"))
            .expect("output symlink");
        assert!(write_requested_format(output_link_project.path(), &report, "markdown").is_err());
        assert_eq!(
            fs::read_to_string(outside_file).expect("outside"),
            "unchanged"
        );
    }
}
