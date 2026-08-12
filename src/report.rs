use crate::model::{EvidenceLevel, Report, ScenarioStatus};
use crate::platform;
use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

const MAX_SAVED_REPORT_BYTES: u64 = 64 * 1_048_576;

pub fn persist(project: &Path, report: &Report, formats: &[String]) -> Result<PathBuf> {
    validate_report_contract(report)?;
    let json = serde_json::to_vec_pretty(report)?;
    validate_artifact_size(json.len())?;
    let markdown = formats
        .iter()
        .any(|format| format == "markdown")
        .then(|| markdown(report).into_bytes());
    if let Some(bytes) = &markdown {
        validate_artifact_size(bytes.len())?;
    }
    let junit = formats
        .iter()
        .any(|format| format == "junit")
        .then(|| junit(report))
        .transpose()?;
    if let Some(bytes) = &junit {
        validate_artifact_size(bytes.len())?;
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
    if let Some(bytes) = &markdown {
        write_new_file(&staging_root.join("report.md"), bytes)?;
    }
    if let Some(bytes) = &junit {
        write_new_file(&staging_root.join("report.junit.xml"), bytes)?;
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
    let report: Report = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!(
            "saved report is not valid report schema v1; parser details are suppressed because report fields may be sensitive"
        )
    })?;
    validate_report_contract(&report)?;
    if report.run_id != run_id {
        bail!("saved report run ID does not match the requested run directory");
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
            escape_control_text(&finding.changed),
            escape_control_text(&finding.observed),
            escape_control_text(&finding.conclusion),
            escape_control_text(&finding.next_step),
            escape_control_text(&finding.not_proven)
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
        escape_markdown(report.platform.get("os").map_or("unknown", String::as_str)),
        escape_markdown(
            report
                .platform
                .get("arch")
                .map_or("unknown", String::as_str)
        ),
        report.workspace_integrity.source_unchanged,
    );
    for scenario in &report.scenarios {
        output.push_str(&format!(
            "| {} | {} | {:?} | {} | {} |\n",
            scenario.id,
            escape_markdown(&scenario.name),
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
            escape_markdown(&finding.changed),
            escape_markdown(&finding.observed),
            escape_markdown(&finding.conclusion),
            escape_markdown(&finding.next_step),
            escape_markdown(&finding.not_proven)
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
    validate_report_contract(report)?;
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
    validate_artifact_size(bytes.len())?;
    atomic_write_regular(&directory, &path, &bytes)?;
    Ok(path)
}

fn validate_artifact_size(bytes: usize) -> Result<()> {
    if bytes as u64 > MAX_SAVED_REPORT_BYTES {
        bail!(
            "generated report artifact exceeds the {} MiB persisted-report limit; reduce run or log budgets",
            MAX_SAVED_REPORT_BYTES / 1_048_576
        );
    }
    Ok(())
}

fn validate_run_id(run_id: &str) -> Result<()> {
    let valid_ulid = run_id
        .parse::<ulid::Ulid>()
        .is_ok_and(|parsed| parsed.to_string() == run_id);
    if !valid_ulid || !platform::is_single_normal_component(run_id) {
        bail!("run ID must be a canonical 26-character ULID and one normal path component");
    }
    Ok(())
}

fn validate_report_contract(report: &Report) -> Result<()> {
    if report.schema_version != 1 {
        bail!("saved report uses an unsupported schema version; actual value suppressed");
    }
    validate_run_id(&report.run_id).context("saved report contains an unsafe run ID")?;
    if !decimal_string(&report.started_at) || !decimal_string(&report.finished_at) {
        bail!("saved report contains an invalid timestamp field; value suppressed");
    }
    if !report.platform.contains_key("os")
        || !report.platform.contains_key("arch")
        || !report.platform.contains_key("family")
    {
        bail!("saved report is missing required platform fields");
    }
    if !lower_hex_64(&report.repository_fingerprint) {
        bail!("saved report contains an invalid repository fingerprint");
    }
    if !matches!(report.configuration.profile.as_str(), "quick" | "deep")
        || report.configuration.timeout_seconds == 0
        || report.configuration.baseline_runs < 2
        || report.configuration.confirm_failures == 0
        || !matches!(
            report.configuration.workspace_mode.as_str(),
            "working-tree" | "git-clean"
        )
    {
        bail!("saved report contains invalid configuration metadata; values suppressed");
    }
    if report.command.is_empty() {
        bail!("saved report command must not be empty");
    }
    if !matches!(
        report.baseline_status.as_str(),
        "STABLE" | "BASELINE_FAILED" | "BASELINE_UNSTABLE"
    ) {
        bail!("saved report contains an invalid baseline status; value suppressed");
    }
    for scenario in &report.scenarios {
        if !stable_id(&scenario.id, "AZ-S") {
            bail!("saved report contains an invalid scenario ID; value suppressed");
        }
    }
    for finding in &report.findings {
        if !stable_id(&finding.id, "AZ-F") || !stable_id(&finding.scenario_id, "AZ-S") {
            bail!("saved report contains an invalid finding ID; value suppressed");
        }
    }
    if report.budget.max_total_runs == 0 || report.budget.max_total_seconds == 0 {
        bail!("saved report contains invalid execution-budget metadata");
    }
    if !lower_hex_64(&report.workspace_integrity.before_fingerprint)
        || !lower_hex_64(&report.workspace_integrity.after_fingerprint)
    {
        bail!("saved report contains an invalid workspace fingerprint");
    }
    Ok(())
}

fn decimal_string(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn stable_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 3 && suffix.bytes().all(|byte| byte.is_ascii_digit()))
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
            escape_control_text(&finding.changed),
            escape_control_text(&finding.observed),
            escape_control_text(&finding.conclusion),
            escape_control_text(&finding.next_step),
            escape_control_text(&finding.not_proven)
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
            let safe = escape_control_text(part);
            if safe.chars().any(char::is_whitespace) {
                format!("{safe:?}")
            } else {
                safe
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_markdown(input: &str) -> String {
    escape_control_text(input).replace('|', "\\|")
}

fn xml_escape(input: &str) -> String {
    let safe = escape_control_text(input);
    let mut output = String::with_capacity(safe.len());
    for character in safe.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ if xml_10_character(character) => output.push(character),
            _ => output.push_str(&format!("[U+{:04X}]", character as u32)),
        }
    }
    output
}

fn escape_control_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        if character.is_control() {
            output.push_str(&format!("[U+{:04X}]", character as u32));
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn terminal_safe(input: &str) -> String {
    escape_control_text(input)
}

const fn xml_10_character(character: char) -> bool {
    matches!(character as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
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
            platform: BTreeMap::from([
                ("os".into(), "test".into()),
                ("arch".into(), "test".into()),
                ("family".into(), "test".into()),
            ]),
            repository_fingerprint: "0".repeat(64),
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
                before_fingerprint: "a".repeat(64),
                after_fingerprint: "a".repeat(64),
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
    fn human_renderers_make_control_characters_visible() {
        let mut report = sample_report(ScenarioStatus::Fail);
        let injected = "before\u{1b}]0;title\u{7}after\nforged";
        report.command = vec![injected.into()];
        report.scenarios[0].name = injected.into();
        report.scenarios[0].note = injected.into();
        report.findings.push(crate::model::Finding {
            id: "AZ-F001".into(),
            scenario_id: "AZ-S001".into(),
            evidence: EvidenceLevel::Confirmed,
            changed: injected.into(),
            observed: injected.into(),
            conclusion: injected.into(),
            next_step: injected.into(),
            not_proven: injected.into(),
            restored_names: vec![],
        });

        let explanation = explain(&report);
        let markdown = markdown(&report);
        for rendered in [explanation, markdown] {
            assert!(!rendered.contains('\u{1b}'));
            assert!(!rendered.contains('\u{7}'));
            assert!(rendered.contains("[U+001B]"));
            assert!(rendered.contains("[U+0007]"));
            assert!(rendered.contains("[U+000A]"));
        }
        let command = display_command(&report.command);
        assert!(!command.contains('\u{1b}'));
        assert!(command.contains("[U+001B]"));
    }

    #[test]
    fn junit_replaces_non_xml_control_characters() {
        let mut report = sample_report(ScenarioStatus::Fail);
        report.scenarios[0].name = "nul\0 one\u{1} esc\u{1b} c1\u{85} bad\u{fffe}".into();
        let xml = String::from_utf8(junit(&report).expect("junit")).expect("UTF-8");
        assert!(!xml.contains('\0'));
        assert!(!xml.contains('\u{1}'));
        assert!(!xml.contains('\u{1b}'));
        assert!(!xml.contains('\u{85}'));
        assert!(!xml.contains('\u{fffe}'));
        assert!(xml.contains("[U+0000]"));
        assert!(xml.contains("[U+0001]"));
        assert!(xml.contains("[U+001B]"));
        assert!(xml.contains("[U+0085]"));
        assert!(xml.contains("[U+FFFE]"));
        assert!(xml.chars().all(xml_10_character));
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
    fn every_generated_report_artifact_uses_the_same_size_limit() {
        assert!(validate_artifact_size(MAX_SAVED_REPORT_BYTES as usize).is_ok());
        assert!(validate_artifact_size(MAX_SAVED_REPORT_BYTES as usize + 1).is_err());
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

    #[test]
    fn loaded_reports_reject_invalid_contracts_without_echoing_values() {
        let project = tempfile::tempdir().expect("project");
        let report = sample_report(ScenarioStatus::Pass);
        let directory = persist(project.path(), &report, &["json".into()]).expect("persist");
        let path = directory.join("report.json");

        let mut unsupported = serde_json::to_value(&report).expect("value");
        unsupported["schema_version"] = serde_json::json!(918273645);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&unsupported).expect("json"),
        )
        .expect("replace fixture");
        let error = load(project.path(), &report.run_id)
            .expect_err("unsupported schema version")
            .to_string();
        assert!(!error.contains("918273645"));

        let mut unknown = serde_json::to_value(&report).expect("value");
        unknown["AZ_INVALID_UNKNOWN_REPORT_FIELD_5511"] = serde_json::json!(true);
        fs::write(&path, serde_json::to_vec_pretty(&unknown).expect("json"))
            .expect("replace fixture");
        let error = load(project.path(), &report.run_id)
            .expect_err("unknown report field")
            .to_string();
        assert!(!error.contains("AZ_INVALID_UNKNOWN_REPORT_FIELD_5511"));
        assert!(error.contains("parser details are suppressed"));

        let mut nested_unknown = serde_json::to_value(&report).expect("value");
        nested_unknown["configuration"]["AZ_INVALID_NESTED_REPORT_FIELD_5512"] =
            serde_json::json!(true);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&nested_unknown).expect("json"),
        )
        .expect("replace fixture");
        let error = load(project.path(), &report.run_id)
            .expect_err("nested unknown report field")
            .to_string();
        assert!(!error.contains("AZ_INVALID_NESTED_REPORT_FIELD_5512"));
        assert!(error.contains("parser details are suppressed"));
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
