use crate::model::{EvidenceLevel, Report, ScenarioStatus};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run_directory(project: &Path, run_id: &str) -> PathBuf {
    project.join(".assumezero").join("runs").join(run_id)
}

pub fn persist(project: &Path, report: &Report, formats: &[String]) -> Result<PathBuf> {
    let directory = run_directory(project, &report.run_id);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "could not create report directory `{}`",
            directory.display()
        )
    })?;
    let json = serde_json::to_vec_pretty(report)?;
    fs::write(directory.join("report.json"), json)?;
    if formats.iter().any(|format| format == "markdown") {
        fs::write(directory.join("report.md"), markdown(report))?;
    }
    if formats.iter().any(|format| format == "junit") {
        fs::write(directory.join("report.junit.xml"), junit(report)?)?;
    }
    Ok(directory)
}

pub fn load(project: &Path, run_id: &str) -> Result<Report> {
    let path = run_directory(project, run_id).join("report.json");
    let bytes = fs::read(&path)
        .with_context(|| format!("run `{run_id}` was not found at `{}`", path.display()))?;
    serde_json::from_slice(&bytes).context("saved report is not valid report schema v1")
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
        "Secret values persisted: no environment values are report fields\nSource workspace unchanged: {}\nRun ID: {}\n",
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
    let directory = run_directory(project, &report.run_id);
    fs::create_dir_all(&directory)?;
    let path = match format {
        "json" => {
            let path = directory.join("report.json");
            fs::write(&path, serde_json::to_vec_pretty(report)?)?;
            path
        }
        "markdown" => {
            let path = directory.join("report.md");
            fs::write(&path, markdown(report))?;
            path
        }
        "junit" => {
            let path = directory.join("report.junit.xml");
            fs::write(&path, junit(report)?)?;
            path
        }
        other => anyhow::bail!("unsupported report format `{other}`"),
    };
    Ok(path)
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
            run_id: "demo".into(),
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
}
