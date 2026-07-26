use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn assumezero() -> Command {
    Command::new(env!("CARGO_BIN_EXE_assumezero"))
}

fn fixture() -> &'static str {
    env!("CARGO_BIN_EXE_assumezero-test-fixture")
}

fn run(command: &mut Command) -> Output {
    command.output().expect("command starts")
}

fn write_config(project: &Path, excluded: &[&str], extra: &str) -> PathBuf {
    let exclusions = excluded
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        r#"version = 1

[run]
timeout_seconds = 5
baseline_runs = 2
confirm_failures = 2

[workspace]
mode = "working-tree"
max_size_mib = 64
exclude = [".git", ".assumezero"]

[oracle]
kind = "exit-code"
accepted_exit_codes = [0]

[scenarios]
profile = "quick"
exclude = [{exclusions}]

[budget]
max_total_runs = 60
max_total_seconds = 120

[report]
formats = ["terminal", "json", "markdown", "junit"]
log_limit_bytes = 32768

{extra}
"#
    );
    let path = project.join("test-config.toml");
    fs::write(&path, text).expect("write config");
    path
}

fn all_except(kept: &str) -> Vec<&'static str> {
    [
        "AZ-S001", "AZ-S002", "AZ-S003", "AZ-S004", "AZ-S005", "AZ-S006", "AZ-S007", "AZ-S008",
        "AZ-S009", "AZ-S010",
    ]
    .into_iter()
    .filter(|id| *id != kept)
    .collect()
}

fn parse_json(output: &Output) -> Value {
    assert!(
        output.status.success()
            || output.status.code() == Some(1)
            || output.status.code() == Some(2),
        "unexpected status {:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON output: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn scan_files(root: &Path) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("read directory") {
            let entry = entry.expect("entry");
            if entry.file_type().expect("type").is_dir() {
                pending.push(entry.path());
            } else {
                result.extend(fs::read(entry.path()).expect("read file"));
            }
        }
    }
    result
}

#[test]
fn init_doctor_help_and_scenario_listing_work() {
    let project = tempfile::tempdir().expect("project");
    let init = run(assumezero().current_dir(project.path()).args(["init"]));
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(project.path().join("assumezero.toml").is_file());

    let existing = run(assumezero().current_dir(project.path()).args(["init"]));
    assert_eq!(existing.status.code(), Some(3));

    let doctor =
        run(assumezero()
            .current_dir(project.path())
            .args(["doctor", "--", fixture(), "pass"]));
    assert!(doctor.status.success());
    let doctor_text = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor_text.contains("No environment values"));

    let help = run(assumezero().arg("--help"));
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("list-scenarios"));

    let list = run(assumezero().args(["--json", "list-scenarios"]));
    let json: Value = serde_json::from_slice(&list.stdout).expect("scenario JSON");
    assert_eq!(json.as_array().map(Vec::len), Some(10));
}

#[test]
fn clean_env_is_minimized_without_persisting_the_value() {
    let project = tempfile::tempdir().expect("project");
    fs::write(project.path().join("source.txt"), "unchanged").expect("source");
    let excludes = all_except("AZ-S003");
    let config = write_config(project.path(), &excludes, "");
    let fake_secret = "ASSUMEZERO_INVALID_TEST_SECRET_7462";
    let output = run(assumezero()
        .current_dir(project.path())
        .env("AZ_REQUIRED_DEMO_TOKEN", fake_secret)
        .args([
            "--json",
            "--config",
            config.to_str().expect("path"),
            "check",
            "--",
            fixture(),
            "required-env",
            "AZ_REQUIRED_DEMO_TOKEN",
        ]));
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = parse_json(&output);
    assert_eq!(report["baseline_status"], "STABLE");
    assert_eq!(report["scenarios"][0]["status"], "FAIL");
    assert_eq!(report["findings"][0]["evidence"], "PROVEN");
    assert_eq!(
        report["findings"][0]["restored_names"][0],
        "AZ_REQUIRED_DEMO_TOKEN"
    );
    assert_eq!(report["workspace_integrity"]["source_unchanged"], true);
    assert_eq!(
        fs::read_to_string(project.path().join("source.txt")).expect("source"),
        "unchanged"
    );
    let persisted = scan_files(&project.path().join(".assumezero"));
    assert!(!String::from_utf8_lossy(&persisted).contains(fake_secret));
}

#[test]
fn empty_home_failure_is_confirmed() {
    let project = tempfile::tempdir().expect("project");
    let fake_home = tempfile::tempdir().expect("home");
    fs::write(
        fake_home.path().join(".assumezero-fixture-config"),
        "fixture",
    )
    .expect("home config");
    let excludes = all_except("AZ-S001");
    let config = write_config(project.path(), &excludes, "");
    let mut command = assumezero();
    command.current_dir(project.path());
    #[cfg(windows)]
    command.env("USERPROFILE", fake_home.path());
    #[cfg(not(windows))]
    command.env("HOME", fake_home.path());
    command.args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "home-config-dependent",
    ]);
    let output = run(&mut command);
    assert_eq!(output.status.code(), Some(1));
    let report = parse_json(&output);
    assert_eq!(report["findings"][0]["scenario_id"], "AZ-S001");
    assert_eq!(report["findings"][0]["evidence"], "CONFIRMED");
}

#[test]
fn minimal_path_finds_hidden_child_tool_directory() {
    let project = tempfile::tempdir().expect("project");
    let hidden = tempfile::tempdir().expect("hidden bin");
    let child = hidden
        .path()
        .join(format!("az-hidden-child{}", std::env::consts::EXE_SUFFIX));
    fs::copy(fixture(), &child).expect("copy helper");
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let joined = std::env::join_paths(
        std::iter::once(hidden.path().to_path_buf()).chain(std::env::split_paths(&current_path)),
    )
    .expect("PATH");
    let excludes = all_except("AZ-S004");
    let config = write_config(
        project.path(),
        &excludes,
        "\n[environment]\npreserve = []\ndeny = []\n",
    );
    let output = run(assumezero()
        .current_dir(project.path())
        .env("PATH", joined)
        .args([
            "--json",
            "--config",
            config.to_str().expect("path"),
            "check",
            "--profile",
            "deep",
            "--",
            fixture(),
            "hidden-path-tool",
            "az-hidden-child",
        ]));
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = parse_json(&output);
    assert_eq!(report["findings"][0]["scenario_id"], "AZ-S004");
    assert_eq!(report["findings"][0]["evidence"], "PROVEN");
    assert_eq!(
        report["findings"][0]["restored_names"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn space_path_failure_and_unstable_baseline_are_distinguished() {
    let space_project = tempfile::tempdir().expect("project");
    let excludes = all_except("AZ-S005");
    let config = write_config(space_project.path(), &excludes, "");
    let space_output = run(assumezero().current_dir(space_project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "fail-on-space",
    ]));
    assert_eq!(space_output.status.code(), Some(1));
    let space_report = parse_json(&space_output);
    assert_eq!(space_report["findings"][0]["scenario_id"], "AZ-S005");

    let flaky_project = tempfile::tempdir().expect("flaky project");
    let state = tempfile::NamedTempFile::new().expect("state");
    fs::write(state.path(), "0").expect("state initial");
    let config = write_config(flaky_project.path(), &[], "");
    let flaky_output = run(assumezero().current_dir(flaky_project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "flaky-baseline",
        state.path().to_str().expect("state path"),
    ]));
    assert_eq!(flaky_output.status.code(), Some(2));
    let flaky_report = parse_json(&flaky_output);
    assert_eq!(flaky_report["baseline_status"], "BASELINE_UNSTABLE");
    assert_eq!(flaky_report["scenarios"].as_array().map(Vec::len), Some(0));
}

#[test]
fn reports_regenerate_as_markdown_json_and_junit() {
    let project = tempfile::tempdir().expect("project");
    let config = write_config(project.path(), &[], "");
    let init = Command::new("git")
        .arg("init")
        .current_dir(project.path())
        .output()
        .expect("git init");
    assert!(init.status.success());
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(project.path())
        .output()
        .expect("git add");
    assert!(add.status.success());
    let output = run(assumezero().current_dir(project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "pass",
    ]));
    assert!(output.status.success());
    let saved = parse_json(&output);
    assert_eq!(saved["workspace_integrity"]["source_unchanged"], true);
    assert_eq!(
        saved["workspace_integrity"]["git_status_before"],
        saved["workspace_integrity"]["git_status_after"]
    );
    let run_id = saved["run_id"].as_str().expect("run id");
    for format in ["markdown", "json", "junit"] {
        let regenerated = run(assumezero()
            .current_dir(project.path())
            .args(["report", run_id, "--format", format]));
        assert!(regenerated.status.success());
    }
    let directory = project.path().join(".assumezero/runs").join(run_id);
    assert!(directory.join("report.md").is_file());
    assert!(directory.join("report.json").is_file());
    assert!(directory.join("report.junit.xml").is_file());
    assert!(
        String::from_utf8_lossy(&fs::read(directory.join("report.junit.xml")).expect("junit"))
            .contains("<testsuite")
    );
}

#[test]
fn timeout_stops_baseline_attribution_and_large_output_is_bounded() {
    let timeout_project = tempfile::tempdir().expect("timeout project");
    let timeout_config = write_config(
        timeout_project.path(),
        &[],
        "\n# timeout is configured above\n",
    );
    let text = fs::read_to_string(&timeout_config)
        .expect("config")
        .replace("timeout_seconds = 5", "timeout_seconds = 1");
    fs::write(&timeout_config, text).expect("config");
    let timeout = run(assumezero().current_dir(timeout_project.path()).args([
        "--json",
        "--config",
        timeout_config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "timeout",
    ]));
    assert_eq!(timeout.status.code(), Some(2));
    let report = parse_json(&timeout);
    assert_eq!(report["baseline_status"], "BASELINE_FAILED");
    assert_eq!(report["baseline"][0]["timed_out"], true);

    let output_project = tempfile::tempdir().expect("output project");
    let output_config = write_config(output_project.path(), &all_except("AZ-S001"), "");
    let large = run(assumezero().current_dir(output_project.path()).args([
        "--json",
        "--config",
        output_config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "large-output",
    ]));
    assert!(large.status.success());
    let report = parse_json(&large);
    assert_eq!(report["baseline"][0]["output_truncated"], true);
    assert!(
        report["baseline"][0]["stdout_summary"]
            .as_str()
            .expect("stdout")
            .len()
            < 40_000
    );
}
