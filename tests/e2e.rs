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

fn all_scenarios() -> Vec<&'static str> {
    [
        "AZ-S001", "AZ-S002", "AZ-S003", "AZ-S004", "AZ-S005", "AZ-S006", "AZ-S007", "AZ-S008",
        "AZ-S009", "AZ-S010",
    ]
    .to_vec()
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

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
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
fn short_sensitive_environment_values_are_redacted_across_outputs_and_reports() {
    let project = tempfile::tempdir().expect("project");
    let config = write_config(project.path(), &all_scenarios(), "");
    let secret_name = "AZ_SHORT_TOKEN";
    let short_secret = "abc";
    let exposed = "fixture-secret=abc";

    let json_output = run(assumezero()
        .current_dir(project.path())
        .env(secret_name, short_secret)
        .args([
            "--json",
            "--config",
            config.to_str().expect("path"),
            "check",
            "--",
            fixture(),
            "secret-output",
            secret_name,
        ]));
    assert!(json_output.status.success());
    let report = parse_json(&json_output);
    let rendered = format!(
        "{}{}{}",
        String::from_utf8_lossy(&json_output.stdout),
        String::from_utf8_lossy(&json_output.stderr),
        serde_json::to_string(&report).expect("report")
    );
    assert!(!rendered.contains(exposed), "{rendered}");
    assert!(rendered.contains("fixture-secret=***"));
    let run_id = report["run_id"].as_str().expect("run id");
    let directory = project.path().join(".assumezero/runs").join(run_id);
    assert!(!String::from_utf8_lossy(&scan_files(&directory)).contains(exposed));

    let terminal_output = run(assumezero()
        .current_dir(project.path())
        .env(secret_name, short_secret)
        .args([
            "--config",
            config.to_str().expect("path"),
            "check",
            "--",
            fixture(),
            "secret-output",
            secret_name,
        ]));
    assert!(terminal_output.status.success());
    let terminal = format!(
        "{}{}",
        String::from_utf8_lossy(&terminal_output.stdout),
        String::from_utf8_lossy(&terminal_output.stderr)
    );
    assert!(!terminal.contains(exposed), "{terminal}");
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

    let outside = tempfile::tempdir().expect("outside report directory");
    let outside_id = outside.path().to_str().expect("outside path");
    let mut malicious = saved.clone();
    malicious["run_id"] = Value::String(outside_id.into());
    fs::write(
        outside.path().join("report.json"),
        serde_json::to_vec_pretty(&malicious).expect("malicious report fixture"),
    )
    .expect("malicious report fixture");
    let escaped = run(assumezero()
        .current_dir(project.path())
        .args(["report", outside_id, "--format", "markdown"]));
    assert_eq!(escaped.status.code(), Some(3));
    assert!(!outside.path().join("report.md").exists());
    for invalid in ["..", "nested/run", "nested\\run", "C:run"] {
        let rejected = run(assumezero()
            .current_dir(project.path())
            .args(["explain", invalid]));
        assert_eq!(rejected.status.code(), Some(3), "{invalid}");
    }
}

#[cfg(unix)]
#[test]
fn prepare_cannot_replace_workspace_root_before_the_tested_command() {
    let project = tempfile::tempdir().expect("project");
    let outside = tempfile::tempdir().expect("outside");
    let marker = outside.path().join("must-not-be-created");
    let config = write_config(project.path(), &[], "");
    let fixture_path = fixture().replace('\\', "\\\\").replace('"', "\\\"");
    let outside_path = outside
        .path()
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let text = fs::read_to_string(&config).expect("config").replace(
        "confirm_failures = 2",
        &format!(
            "confirm_failures = 2\nprepare = [[\"{fixture_path}\", \"replace-cwd-with-symlink\", \"{outside_path}\"]]"
        ),
    );
    fs::write(&config, text).expect("config");

    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("config path"),
        "check",
        "--",
        fixture(),
        "create-file",
        marker.to_str().expect("marker"),
    ]));
    assert_eq!(output.status.code(), Some(3));
    assert!(!marker.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("isolated project root"));
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

#[test]
fn cli_only_secrets_are_redacted_across_outputs_and_reports() {
    let project = tempfile::tempdir().expect("project");
    let initial_config = write_config(
        project.path(),
        &all_scenarios(),
        "sensitive_options = [\"-p\"]\n",
    );
    let sentinel_token = "AZ_INVALID_CLI_TOKEN_4101";
    let sentinel_password = "AZ_INVALID_CLI_PASSWORD_4102";
    let sentinel_api_key = "AZ_INVALID_CLI_API_KEY_4103";
    let sentinel_short = "AZ_INVALID_CLI_SHORT_4104";
    let sentinel_prepare = "AZ_INVALID_CLI_PREPARE_4105";
    let sentinel_github = "AZ_INVALID_CLI_GITHUB_4106";
    let sentinel_attached_short = "AZ_INVALID_CLI_ATTACHED_SHORT_4107";
    let attached_short = format!("-p{sentinel_attached_short}");
    let config_text = fs::read_to_string(&initial_config)
        .expect("config")
        .replace(
            "confirm_failures = 2",
            &format!(
                "confirm_failures = 2\nprepare = [[{}, \"echo-args\", \"--token\", \"{sentinel_prepare}\"]]",
                toml_string(fixture())
            ),
        )
        .replace(
            "accepted_exit_codes = [0]",
            &format!("accepted_exit_codes = [0]\nstdout_contains = [\"{sentinel_token}\"]"),
        );
    fs::write(&initial_config, config_text).expect("config");
    let config = project.path().join(sentinel_token);
    fs::rename(initial_config, &config).expect("secret-named config fixture");
    let init = Command::new("git")
        .arg("init")
        .current_dir(project.path())
        .output()
        .expect("git init");
    assert!(init.status.success());
    let tested_command = [
        fixture(),
        "echo-args",
        "--token",
        sentinel_token,
        "--Password",
        sentinel_password,
        &format!("--api-key={sentinel_api_key}"),
        "-p",
        sentinel_short,
        attached_short.as_str(),
        "--github-token",
        sentinel_github,
        "--output",
        "ordinary-artifact",
    ];

    for collision in ["quick", "STABLE", "os"] {
        let collision_config = write_config(project.path(), &all_scenarios(), "");
        let collision_output = run(assumezero().current_dir(project.path()).args([
            "--json",
            "--config",
            collision_config.to_str().expect("path"),
            "check",
            "--",
            fixture(),
            "pass",
            "--token",
            collision,
        ]));
        assert!(collision_output.status.success(), "{collision}");
        let collision_report = parse_json(&collision_output);
        assert_eq!(collision_report["configuration"]["profile"], "quick");
        assert_eq!(collision_report["baseline_status"], "STABLE");
        assert!(collision_report["platform"].get("os").is_some());
    }

    for json in [false, true] {
        let mut dry_run = assumezero();
        dry_run.current_dir(project.path());
        if json {
            dry_run.arg("--json");
        }
        dry_run
            .args([
                "--config",
                config.to_str().expect("path"),
                "check",
                "--dry-run",
                "--",
            ])
            .args(tested_command);
        let output = run(&mut dry_run);
        assert!(output.status.success());
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for sentinel in [
            sentinel_token,
            sentinel_password,
            sentinel_api_key,
            sentinel_short,
            sentinel_prepare,
            sentinel_github,
            sentinel_attached_short,
        ] {
            assert!(!rendered.contains(sentinel));
        }
        assert!(rendered.contains("ordinary-artifact"));
    }

    for json in [false, true] {
        let mut dry_run = assumezero();
        dry_run.current_dir(project.path());
        if json {
            dry_run.arg("--json");
        }
        dry_run
            .args([
                "--config",
                config.to_str().expect("path"),
                "check",
                "--dry-run",
                "--shell",
                "--",
            ])
            .args(tested_command);
        let output = run(&mut dry_run);
        assert!(output.status.success());
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for sentinel in [
            sentinel_token,
            sentinel_password,
            sentinel_api_key,
            sentinel_short,
            sentinel_prepare,
            sentinel_github,
            sentinel_attached_short,
        ] {
            assert!(!rendered.contains(sentinel));
        }
        assert!(rendered.contains("ordinary-artifact"));
    }

    let mut check = assumezero();
    check
        .current_dir(project.path())
        .args([
            "--json",
            "--verbose",
            "--config",
            config.to_str().expect("path"),
            "check",
            "--",
        ])
        .args(tested_command);
    let output = run(&mut check);
    assert!(output.status.success());
    let report = parse_json(&output);
    assert!(report["command"]
        .as_array()
        .expect("command")
        .iter()
        .any(|value| value == "ordinary-artifact"));
    let rendered = format!(
        "{}{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        serde_json::to_string(&report).expect("report")
    );
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!rendered.contains(sentinel));
    }
    assert!(rendered.contains("REDACTED_CLI_VALUE"));

    let run_id = report["run_id"].as_str().expect("run id").to_owned();
    let directory = project.path().join(".assumezero/runs").join(&run_id);
    let persisted = scan_files(&directory);
    let persisted = String::from_utf8_lossy(&persisted);
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!persisted.contains(sentinel));
    }
    assert!(persisted.contains("ordinary-artifact"));

    let mut shell_check = assumezero();
    shell_check
        .current_dir(project.path())
        .args([
            "--json",
            "--verbose",
            "--config",
            config.to_str().expect("path"),
            "check",
            "--shell",
            "--",
        ])
        .args(tested_command);
    let shell_output = run(&mut shell_check);
    assert!(shell_output.status.success());
    let shell_report = parse_json(&shell_output);
    let rendered = format!(
        "{}{}{}",
        String::from_utf8_lossy(&shell_output.stdout),
        String::from_utf8_lossy(&shell_output.stderr),
        serde_json::to_string(&shell_report).expect("report")
    );
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!rendered.contains(sentinel));
    }
    assert!(rendered.contains("ordinary-artifact"));
    let shell_run_id = shell_report["run_id"].as_str().expect("run id");
    let shell_directory = project.path().join(".assumezero/runs").join(shell_run_id);
    let shell_persisted = String::from_utf8_lossy(&scan_files(&shell_directory)).into_owned();
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!shell_persisted.contains(sentinel));
    }

    let terminal_output = run(assumezero()
        .current_dir(project.path())
        .args(["--config", config.to_str().expect("path"), "check", "--"])
        .args(tested_command));
    assert!(terminal_output.status.success());
    let terminal_rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&terminal_output.stdout),
        String::from_utf8_lossy(&terminal_output.stderr)
    );
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!terminal_rendered.contains(sentinel));
    }
    assert!(terminal_rendered.contains("AssumeZero completed"));

    for format in ["markdown", "json", "junit"] {
        let regenerated = run(assumezero().current_dir(project.path()).args([
            "report",
            run_id.as_str(),
            "--format",
            format,
        ]));
        assert!(regenerated.status.success());
    }
    let persisted = String::from_utf8_lossy(&scan_files(&directory)).into_owned();
    for sentinel in [
        sentinel_token,
        sentinel_password,
        sentinel_api_key,
        sentinel_short,
        sentinel_prepare,
        sentinel_github,
        sentinel_attached_short,
    ] {
        assert!(!persisted.contains(sentinel));
    }
}

#[test]
fn prepare_failure_error_redacts_sensitive_command_values() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_PREPARE_FAILURE_4201";
    let config = write_config(project.path(), &all_scenarios(), "");
    let text = fs::read_to_string(&config).expect("config").replace(
        "confirm_failures = 2",
        &format!(
            "confirm_failures = 2\nprepare = [[{}, \"fail\", \"--token\", \"{sentinel}\"]]",
            toml_string(fixture())
        ),
    );
    fs::write(&config, text).expect("config");
    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "pass",
    ]));
    assert_eq!(output.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    assert!(rendered.contains("REDACTED_CLI_VALUE"));
}

#[test]
fn configuration_parse_errors_do_not_echo_sensitive_source_lines() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_CONFIG_PARSE_4301";
    let config = project.path().join("invalid.toml");
    fs::write(
        &config,
        format!(
            "version = 1\n[run]\ncommand = [\"tool\", \"--token\", \"{sentinel}\"]\ninvalid = \"\\q\"\n"
        ),
    )
    .expect("invalid config");
    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("path"),
        "check",
    ]));
    assert_eq!(output.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!rendered.contains(sentinel));
    assert!(rendered.contains("source excerpts are suppressed"));
}

#[test]
fn configuration_validation_errors_suppress_sensitive_patterns_and_options() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_CONFIG_VALIDATE_4302";
    let config = project.path().join("invalid-validation.toml");
    fs::write(
        &config,
        format!(
            "version = 1\n[run]\ncommand = [\"tool\", \"--token\", \"{sentinel}\"]\n[oracle]\nstdout_regex = \"{sentinel}(\"\n[report]\nsensitive_options = [\"--token={sentinel}\"]\n"
        ),
    )
    .expect("invalid config");
    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("path"),
        "check",
    ]));
    assert_eq!(output.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    assert!(rendered.contains("pattern suppressed"));
}

#[test]
fn configuration_unknown_values_do_not_reintroduce_command_secrets() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_CONFIG_FIELD_4303";
    let config = project.path().join("invalid-field.toml");
    fs::write(
        &config,
        format!(
            "version = 1\n[run]\ncommand = [\"tool\", \"--token\", \"{sentinel}\"]\n[scenarios]\ninclude = [\"{sentinel}\"]\n"
        ),
    )
    .expect("invalid config");
    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("path"),
        "check",
    ]));
    assert_eq!(output.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    assert!(rendered.contains("value suppressed"));
}

#[test]
fn configuration_version_errors_do_not_reintroduce_command_secrets() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "918273645";
    let config = project.path().join("invalid-version.toml");
    fs::write(
        &config,
        format!("version = {sentinel}\n[run]\ncommand = [\"tool\", \"--token\", \"{sentinel}\"]\n"),
    )
    .expect("invalid config");
    let output = run(assumezero().current_dir(project.path()).args([
        "--config",
        config.to_str().expect("path"),
        "check",
    ]));
    assert_eq!(output.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    assert!(rendered.contains("actual value suppressed"));
}

#[test]
fn sensitive_values_do_not_reenter_through_project_paths() {
    let project = tempfile::tempdir().expect("project");
    let config = write_config(project.path(), &all_scenarios(), "");
    let sentinel = "AZ_INVALID_SECRET_PATH_4304";
    let absolute_secret_path = project.path().join(sentinel);
    let output = run(assumezero().current_dir(project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--",
        fixture(),
        "echo-args",
        "--token",
        sentinel,
        absolute_secret_path.to_str().expect("secret path"),
    ]));
    assert!(output.status.success());
    let report = parse_json(&output);
    let rendered = format!(
        "{}{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        serde_json::to_string(&report).expect("report")
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    let run_id = report["run_id"].as_str().expect("run id");
    let directory = project.path().join(".assumezero/runs").join(run_id);
    assert!(!String::from_utf8_lossy(&scan_files(&directory)).contains(sentinel));
}

#[test]
fn historical_report_regeneration_reapplies_builtin_redaction() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_OLD_REPORT_4401";
    let config = write_config(project.path(), &all_scenarios(), "");
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
    let report = parse_json(&output);
    let run_id = report["run_id"].as_str().expect("run id").to_owned();
    let directory = project.path().join(".assumezero/runs").join(&run_id);
    let mut historical = report;
    historical["command"] = serde_json::json!(["tool", "--token", sentinel]);
    historical["tool_version"] = Value::String(sentinel.into());
    historical["platform"]["os"] = Value::String(sentinel.into());
    historical["platform"]
        .as_object_mut()
        .expect("platform")
        .insert(sentinel.into(), Value::String("custom".into()));
    historical["configuration"]["report_formats"] = serde_json::json!([sentinel]);
    historical["redaction_summary"] = Value::Object(
        [(sentinel.to_string(), Value::from(1))]
            .into_iter()
            .collect(),
    );
    historical["workspace_integrity"]["note"] = Value::String(sentinel.into());
    historical["scenarios"] = serde_json::json!([{
        "id": "AZ-S001",
        "name": sentinel,
        "description": sentinel,
        "status": "FAIL",
        "best_effort": false,
        "runs": [],
        "restored_names": [],
        "minimization_complete": false,
        "note": sentinel
    }]);
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&historical).expect("historical fixture"),
    )
    .expect("historical fixture");

    let mut regenerated_output = Vec::new();
    for format in ["markdown", "junit", "json"] {
        let regenerated = run(assumezero().current_dir(project.path()).args([
            "report",
            run_id.as_str(),
            "--format",
            format,
        ]));
        assert!(regenerated.status.success(), "{format}");
        regenerated_output.extend(regenerated.stdout);
        regenerated_output.extend(regenerated.stderr);
    }
    let explained =
        run(assumezero()
            .current_dir(project.path())
            .args(["--json", "explain", run_id.as_str()]));
    assert!(explained.status.success());
    let persisted = scan_files(&directory);
    let rendered = format!(
        "{}{}{}",
        String::from_utf8_lossy(&regenerated_output),
        String::from_utf8_lossy(&explained.stdout),
        String::from_utf8_lossy(&persisted)
    );
    assert!(!rendered.contains(sentinel), "{rendered}");
    assert!(rendered.contains("REDACTED_CLI_VALUE"));
}

#[test]
fn legacy_raw_shell_reports_are_refused_without_echoing_or_rewriting() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_LEGACY_SHELL_4402";
    let config = write_config(project.path(), &all_scenarios(), "");
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
    let mut report = parse_json(&output);
    let run_id = report["run_id"].as_str().expect("run id").to_owned();
    let directory = project.path().join(".assumezero/runs").join(&run_id);
    report["command"] = serde_json::json!([
        "<ABSOLUTE_PATH>/sh",
        "-c",
        format!("echo --token {sentinel}")
    ]);
    report["baseline"][0]["stdout_summary"] = Value::String(sentinel.into());
    let report_path = directory.join("report.json");
    let raw = serde_json::to_vec_pretty(&report).expect("legacy raw shell report");
    fs::write(&report_path, &raw).expect("replace report");
    for artifact in ["report.md", "report.junit.xml"] {
        let path = directory.join(artifact);
        if path.exists() {
            fs::remove_file(path).expect("remove prior artifact");
        }
    }

    for arguments in [
        vec!["explain", run_id.as_str()],
        vec!["report", run_id.as_str(), "--format", "markdown"],
        vec!["report", run_id.as_str(), "--format", "junit"],
        vec!["report", run_id.as_str(), "--format", "json"],
    ] {
        let rejected = run(assumezero().current_dir(project.path()).args(arguments));
        assert_eq!(rejected.status.code(), Some(3));
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&rejected.stdout),
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert!(!rendered.contains(sentinel), "{rendered}");
        assert!(rendered.contains("saved report contains a shell wrapper"));
    }
    assert_eq!(fs::read(&report_path).expect("original report"), raw);
    assert!(!directory.join("report.md").exists());
    assert!(!directory.join("report.junit.xml").exists());

    report["command"] =
        serde_json::json!(["<ABSOLUTE_PATH>/sh", "-c", "<REDACTED_OPAQUE_SHELL_SCRIPT>"]);
    let marked = serde_json::to_vec_pretty(&report).expect("marked legacy shell report");
    fs::write(&report_path, &marked).expect("replace marked report");
    for arguments in [
        vec!["explain", run_id.as_str()],
        vec!["report", run_id.as_str(), "--format", "markdown"],
        vec!["report", run_id.as_str(), "--format", "junit"],
        vec!["report", run_id.as_str(), "--format", "json"],
    ] {
        let rejected = run(assumezero().current_dir(project.path()).args(arguments));
        assert_eq!(rejected.status.code(), Some(3));
        let rendered = format!(
            "{}{}",
            String::from_utf8_lossy(&rejected.stdout),
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert!(!rendered.contains(sentinel), "{rendered}");
        assert!(!rendered.contains("REDACTED_OPAQUE_SHELL_SCRIPT"));
    }
    assert_eq!(fs::read(&report_path).expect("marked report"), marked);
    assert!(!directory.join("report.md").exists());
    assert!(!directory.join("report.junit.xml").exists());
}

#[test]
fn explain_escapes_control_characters_from_saved_reports() {
    let project = tempfile::tempdir().expect("project");
    let config = write_config(project.path(), &all_scenarios(), "");
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
    let mut report = parse_json(&output);
    let run_id = report["run_id"].as_str().expect("run id").to_owned();
    let directory = project.path().join(".assumezero/runs").join(&run_id);
    report["findings"] = serde_json::json!([{
        "id": "AZ-F001",
        "scenario_id": "AZ-S001",
        "evidence": "CONFIRMED",
        "changed": "before\u{001b}]0;title\u{0007}after",
        "observed": "ordinary",
        "conclusion": "ordinary",
        "next_step": "ordinary",
        "not_proven": "ordinary",
        "restored_names": []
    }]);
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&report).expect("control report"),
    )
    .expect("replace report");

    let explained = run(assumezero()
        .current_dir(project.path())
        .args(["explain", run_id.as_str()]));
    assert!(explained.status.success());
    assert!(!explained.stdout.contains(&0x1b));
    assert!(!explained.stdout.contains(&0x07));
    let rendered = String::from_utf8_lossy(&explained.stdout);
    assert!(rendered.contains("[U+001B]"));
    assert!(rendered.contains("[U+0007]"));
}

#[test]
fn opaque_shell_scripts_are_refused_without_echoing_the_script() {
    let project = tempfile::tempdir().expect("project");
    let sentinel = "AZ_INVALID_OPAQUE_SHELL_4501";
    let config = write_config(project.path(), &all_scenarios(), "");
    let script = format!("{} echo-args --token {sentinel}", fixture());

    let dry_run = run(assumezero().current_dir(project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--dry-run",
        "--shell",
        "--",
        &script,
    ]));
    assert_eq!(dry_run.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&dry_run.stdout),
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!rendered.contains(sentinel));
    assert!(rendered.contains("single opaque `--shell` script is refused"));

    let check = run(assumezero().current_dir(project.path()).args([
        "--json",
        "--config",
        config.to_str().expect("path"),
        "check",
        "--shell",
        "--",
        &script,
    ]));
    assert_eq!(check.status.code(), Some(3));
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(!rendered.contains(sentinel));
    assert!(!project.path().join(".assumezero").exists());
}
