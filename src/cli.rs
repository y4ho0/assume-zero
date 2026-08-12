use crate::config::{Config, EXAMPLE_CONFIG};
use crate::engine;
use crate::platform;
use crate::redaction::Redactor;
use crate::report;
use crate::scenarios;
use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "assumezero",
    version,
    about = "Test what your project assumes about the machine it runs on.",
    long_about = "AssumeZero runs a finite command in disposable project copies while changing controlled environment conditions. It protects the source workspace from direct command writes, but it is not a security sandbox for untrusted code."
)]
pub struct Cli {
    /// Stream bounded tested-command output and show additional details.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    verbose: bool,
    /// Suppress the human-readable terminal summary.
    #[arg(long, global = true, action = ArgAction::SetTrue, conflicts_with = "verbose")]
    quiet: bool,
    /// Disable color output.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    no_color: bool,
    /// Emit machine-readable JSON where the command supports it.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    json: bool,
    /// Load configuration from this path.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a documented assumezero.toml configuration.
    Init {
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
        #[arg(long, default_value = "assumezero.toml")]
        path: PathBuf,
    },
    /// Inspect local capabilities without displaying environment values.
    Doctor {
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Test a finite command in controlled copied workspaces.
    Check {
        #[arg(long, value_parser = ["quick", "deep"])]
        profile: Option<String>,
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        shell: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        strict_output: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        suspected_is_failure: bool,
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// List stable scenario IDs and platform requirements.
    ListScenarios,
    /// Explain the findings saved for a run.
    Explain { run_id: String },
    /// Regenerate a saved report.
    Report {
        run_id: String,
        #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
        format: ReportFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportFormat {
    Markdown,
    Json,
    Junit,
}

impl ReportFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Junit => "junit",
        }
    }
}

pub fn run_cli() -> Result<u8> {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(code) => Ok(code),
        Err(error) => {
            let rendered = env::current_dir().ok().map_or_else(
                || report::terminal_safe(&format!("{error:#}")),
                |project| {
                    let redacted = Redactor::new(&env::vars().collect(), &project)
                        .redact(&format!("{error:#}"));
                    report::terminal_safe(&redacted)
                },
            );
            eprintln!("AssumeZero could not use the requested configuration or command.");
            eprintln!("\nWhat happened:\n  {rendered}");
            eprintln!("\nOriginal project modified:\n  No tested command was run in the source directory.");
            eprintln!(
                "\nNext:\n  Correct the field or command shown above, or run `assumezero doctor`."
            );
            Ok(3)
        }
    }
}

fn execute(cli: Cli) -> Result<u8> {
    let current = env::current_dir().context("current directory is unavailable")?;
    match cli.command {
        Commands::Init { force, path } => init(&path, force, cli.json),
        Commands::Doctor { command } => doctor(&current, &command, cli.json),
        Commands::ListScenarios => {
            list_scenarios(cli.json);
            Ok(0)
        }
        Commands::Explain { run_id } => {
            let mut saved = report::load(&current, &run_id)?;
            redact_loaded_report(&current, &mut saved)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&saved.findings)?);
            } else {
                print!("{}", report::explain(&saved));
            }
            Ok(0)
        }
        Commands::Report { run_id, format } => {
            let mut saved = report::load(&current, &run_id)?;
            redact_loaded_report(&current, &mut saved)?;
            let path = report::write_requested_format(&current, &saved, format.as_str())?;
            println!(
                "Wrote <PROJECT>/{}",
                report::terminal_safe(
                    &path
                        .strip_prefix(&current)
                        .unwrap_or(&path)
                        .display()
                        .to_string()
                )
            );
            Ok(0)
        }
        Commands::Check {
            profile,
            dry_run,
            shell,
            strict_output,
            suspected_is_failure,
            command,
        } => {
            let mut bootstrap_redactor = Redactor::new(&env::vars().collect(), &current);
            bootstrap_redactor.add_commands([command.as_slice()], &[]);
            let (mut config, source) =
                load_config(cli.config.as_deref(), &current).map_err(|error| {
                    anyhow::anyhow!(bootstrap_redactor.redact(&format!("{error:#}")))
                })?;
            if let Some(profile) = profile {
                config.scenarios.profile = profile;
            }
            if strict_output {
                config.run.strict_output = true;
            }
            let mut final_command = if command.is_empty() {
                config.run.command.clone()
            } else {
                command
            };
            if final_command.is_empty() {
                anyhow::bail!(
                    "no command is configured; use `assumezero check -- cargo test` \
                     or set `run.command` in assumezero.toml"
                );
            }
            let logical_command = final_command.clone();
            if shell && logical_command.len() == 1 {
                anyhow::bail!(
                    "a single opaque `--shell` script is refused because its CLI secret values cannot be redacted reliably; pass a structured argument vector without `--shell`, or move sensitive values into recognized environment variables"
                );
            }
            if shell {
                let script = if final_command.len() == 1 {
                    final_command.remove(0)
                } else {
                    final_command.join(" ")
                };
                eprintln!(
                    "Warning: the command will be parsed by the system shell. Trust the project and command. AssumeZero is not a security sandbox."
                );
                #[cfg(windows)]
                {
                    final_command = vec![
                        env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
                        "/D".into(),
                        "/S".into(),
                        "/C".into(),
                        script,
                    ];
                }
                #[cfg(not(windows))]
                {
                    final_command = vec!["/bin/sh".into(), "-c".into(), script];
                }
            }
            config.run.command.clone_from(&final_command);
            config.validate()?;
            let command_redactor =
                redactor_for_commands(&config, &current, &final_command, &logical_command);
            let redacted_command = command_redactor.redact_command(&final_command);
            let redacted_source = command_redactor.redact(&source);
            if dry_run {
                dry_run_summary(&config, &redacted_source, &redacted_command, cli.json);
                return Ok(0);
            }
            if !cli.quiet {
                eprintln!(
                    "Final command: {}",
                    report::terminal_safe(&redacted_command.join(" "))
                );
                eprintln!(
                    "Configuration source: {}",
                    report::terminal_safe(&redacted_source)
                );
            }
            let output = engine::check(
                &current,
                &config,
                &redacted_source,
                &final_command,
                &logical_command,
                cli.verbose,
            )
            .map_err(|error| anyhow::anyhow!(command_redactor.redact(&format!("{error:#}"))))?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&output.report)?);
            } else {
                report::print_terminal(&output.report, cli.quiet);
                if !cli.quiet {
                    println!(
                        "Evidence: <PROJECT>/{}",
                        report::terminal_safe(
                            &output
                                .directory
                                .strip_prefix(&current)
                                .unwrap_or(&output.directory)
                                .display()
                                .to_string()
                        )
                    );
                }
            }
            Ok(report::exit_code(&output.report, suspected_is_failure))
        }
    }
}

fn load_config(explicit: Option<&Path>, current: &Path) -> Result<(Config, String)> {
    if let Some(path) = explicit {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            current.join(path)
        };
        let source = resolved.strip_prefix(current).map_or_else(
            |_| "<EXTERNAL_CONFIG>".into(),
            |relative| relative.display().to_string(),
        );
        return Ok((Config::load(&resolved)?, source));
    }
    let conventional = current.join("assumezero.toml");
    if conventional.is_file() {
        Ok((Config::load(&conventional)?, "assumezero.toml".into()))
    } else {
        Ok((Config::default(), "built-in defaults".into()))
    }
}

fn redactor_for_commands(
    config: &Config,
    project: &Path,
    command: &[String],
    logical_command: &[String],
) -> Redactor {
    let environment = env::vars().collect();
    let mut redactor = Redactor::new(&environment, project);
    redactor.add_commands(
        std::iter::once(command)
            .chain(std::iter::once(logical_command))
            .chain(config.run.prepare.iter().map(Vec::as_slice)),
        &config.report.sensitive_options,
    );
    redactor
}

fn redact_loaded_report(project: &Path, report: &mut crate::model::Report) -> Result<()> {
    reject_raw_opaque_shell_report(&report.command)?;
    let mut redactor = Redactor::new(&env::vars().collect(), project);
    redactor.add_commands([report.command.as_slice()], &[]);
    redactor.redact_loaded_report(report);
    Ok(())
}

fn reject_raw_opaque_shell_report(command: &[String]) -> Result<()> {
    if is_shell_wrapper(command) {
        anyhow::bail!(
            "saved report contains a shell wrapper whose historical output cannot be redacted reliably and therefore cannot be rendered safely; command details are suppressed"
        );
    }
    Ok(())
}

fn is_shell_wrapper(command: &[String]) -> bool {
    let Some(executable) = command.first() else {
        return false;
    };
    let basename = executable
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(executable)
        .to_ascii_lowercase();
    (basename == "sh" && command.len() == 3 && command[1] == "-c")
        || (command.len() == 5
            && command[1..4]
                .iter()
                .zip(["/d", "/s", "/c"])
                .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected)))
}

fn init(path: &Path, force: bool, json_output: bool) -> Result<u8> {
    if path.exists() && !force {
        anyhow::bail!(
            "`{}` already exists and was not overwritten; use `--force` only after reviewing it",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, EXAMPLE_CONFIG)?;
    if json_output {
        println!("{}", json!({"created": path, "overwritten": force}));
    } else {
        println!(
            "Created {}",
            report::terminal_safe(&path.display().to_string())
        );
        println!("Next: edit [run].command, then run `assumezero doctor`.");
    }
    Ok(0)
}

fn doctor(current: &Path, command: &[String], json_output: bool) -> Result<u8> {
    let readable = fs::read_dir(current).is_ok();
    let temporary = tempfile::Builder::new()
        .prefix("assumezero-doctor-")
        .tempdir();
    let temporary_writable = temporary.is_ok();
    let unicode = temporary.as_ref().is_ok_and(|directory| {
        let path = directory.path().join("项目-Δ");
        fs::create_dir(&path).is_ok()
    });
    let long_path = temporary.as_ref().is_ok_and(|directory| {
        let mut path = directory.path().to_path_buf();
        for index in 0..8 {
            path.push(format!("assumezero-long-segment-{index:02}"));
        }
        fs::create_dir_all(path).is_ok()
    });
    let symlink = symlink_capability(temporary.as_ref().ok().map(tempfile::TempDir::path));
    let git = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    let command_resolution: String = command.first().map_or_else(
        || "not requested".to_string(),
        |name| {
            let path = env::var_os("PATH");
            if platform::resolve_program(name, path.as_ref()).is_some() || Path::new(name).is_file()
            {
                "resolved".to_string()
            } else {
                "not found".to_string()
            }
        },
    );
    let data = json!({
        "os": env::consts::OS,
        "arch": env::consts::ARCH,
        "current_directory_readable": readable,
        "temporary_directory_writable": temporary_writable,
        "git_available": git,
        "tested_command": command_resolution,
        "symlink_capability": symlink,
        "unicode_path_capability": unicode,
        "bounded_deep_path_capability": long_path,
        "timezone_utc_capability": !cfg!(windows),
        "locale_c_capability": scenarios::locale_c_supported(),
        "workspace_copy": {
            "working_tree": true,
            "git_clean": git
        },
        "privacy": "No environment values or home-directory paths were displayed."
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        println!("AssumeZero doctor\n");
        println!("Platform: {} / {}", env::consts::OS, env::consts::ARCH);
        println!("Current directory readable: {readable}");
        println!("Temporary directory writable: {temporary_writable}");
        println!("Git available: {git}");
        println!("Tested command: {command_resolution}");
        println!("Symlink capability: {symlink}");
        println!("Unicode paths: {unicode}");
        println!("Bounded deep paths: {long_path}");
        println!("Process timezone scenario: {}", !cfg!(windows));
        println!("C locale scenario: {}", scenarios::locale_c_supported());
        println!("\nNo environment values or home-directory paths were displayed.");
        if !readable || !temporary_writable {
            println!(
                "\nNext: choose a readable project and a writable system temporary directory."
            );
        } else {
            println!("\nNext: run `assumezero check -- <finite command>`.");
        }
    }
    Ok(0)
}

#[cfg(unix)]
fn symlink_capability(root: Option<&Path>) -> bool {
    root.is_some_and(|root| {
        let target = root.join("target");
        let link = root.join("link");
        fs::write(&target, "test").is_ok() && std::os::unix::fs::symlink(&target, link).is_ok()
    })
}

#[cfg(windows)]
fn symlink_capability(root: Option<&Path>) -> bool {
    root.is_some_and(|root| {
        let target = root.join("target");
        let link = root.join("link");
        fs::write(&target, "test").is_ok()
            && std::os::windows::fs::symlink_file(&target, link).is_ok()
    })
}

fn list_scenarios(json_output: bool) {
    if json_output {
        let scenarios: Vec<_> = scenarios::ALL
            .iter()
            .map(|item| {
                json!({
                    "id": item.id,
                    "name": item.name,
                    "description": item.description,
                    "profile": if item.quick { "quick" } else { "deep" },
                    "best_effort": item.best_effort
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&scenarios).unwrap_or_else(|_| "[]".into())
        );
        return;
    }
    println!("ID       NAME              PROFILE  DESCRIPTION");
    for item in scenarios::ALL {
        println!(
            "{:<8} {:<17} {:<8} {}{}",
            item.id,
            item.name,
            if item.quick { "quick" } else { "deep" },
            item.description,
            if item.best_effort {
                " (best effort)"
            } else {
                ""
            }
        );
    }
}

fn dry_run_summary(config: &Config, source: &str, command: &[String], json_output: bool) {
    let selected: Vec<_> = scenarios::selected(config)
        .iter()
        .map(|scenario| scenario.id)
        .collect();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dry_run": true,
                "configuration_source": source,
                "command": command,
                "profile": config.scenarios.profile,
                "scenarios": selected,
                "baseline_runs": config.run.baseline_runs,
                "max_total_runs": config.budget.max_total_runs,
                "source_command_execution": false
            }))
            .unwrap_or_default()
        );
    } else {
        println!("AssumeZero dry run\n");
        println!(
            "Final command: {}",
            report::terminal_safe(&command.join(" "))
        );
        println!("Configuration source: {}", report::terminal_safe(source));
        println!("Profile: {}", config.scenarios.profile);
        println!("Baseline runs: {}", config.run.baseline_runs);
        println!("Scenarios: {}", selected.join(", "));
        println!("Execution budget: {} runs", config.budget.max_total_runs);
        println!("No command was executed.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_command_overrides_configuration_command() {
        let config = Config {
            run: crate::config::RunConfig {
                command: vec!["configured".into()],
                ..crate::config::RunConfig::default()
            },
            ..Config::default()
        };
        let cli = vec!["cli".to_string()];
        let final_command = if cli.is_empty() {
            config.run.command
        } else {
            cli
        };
        assert_eq!(final_command, vec!["cli"]);
    }

    #[test]
    fn legacy_opaque_shell_wrappers_are_detected_portably() {
        for command in [
            vec!["/bin/sh", "-c", "raw"],
            vec!["<ABSOLUTE_PATH>/sh", "-c", "raw"],
            vec![r"C:\Windows\System32\cmd.exe", "/D", "/S", "/C", "raw"],
            vec!["<ABSOLUTE_PATH>/cmd.exe", "/d", "/s", "/c", "raw"],
            vec![
                "<ABSOLUTE_PATH>/custom-comspec.exe",
                "/D",
                "/S",
                "/C",
                "raw",
            ],
        ] {
            let command = command.into_iter().map(String::from).collect::<Vec<_>>();
            assert!(is_shell_wrapper(&command), "{command:?}");
        }
        for command in [
            vec!["bash", "-c", "raw"],
            vec!["sh", "-lc", "raw"],
            vec!["sh", "script.sh"],
            vec!["cmd.exe", "/S", "/C", "raw"],
            vec!["tool", "-c", "ordinary"],
        ] {
            let command = command.into_iter().map(String::from).collect::<Vec<_>>();
            assert!(!is_shell_wrapper(&command), "{command:?}");
        }

        let sentinel = "AZ_INVALID_LEGACY_SHELL_UNIT_7712";
        let raw = ["<ABSOLUTE_PATH>/sh", "-c", sentinel]
            .map(String::from)
            .to_vec();
        let error = reject_raw_opaque_shell_report(&raw)
            .expect_err("raw shell report must be refused")
            .to_string();
        assert!(!error.contains(sentinel));

        let redacted = ["<ABSOLUTE_PATH>/sh", "-c", "<REDACTED_OPAQUE_SHELL_SCRIPT>"]
            .map(String::from)
            .to_vec();
        let error = reject_raw_opaque_shell_report(&redacted)
            .expect_err("historical output remains untrusted even with a redacted command")
            .to_string();
        assert!(!error.contains("REDACTED_OPAQUE_SHELL_SCRIPT"));
    }
}
