use crate::config::OracleConfig;
use crate::model::{OracleCheck, RawExecution, RunEvidence};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;

pub fn evaluate(
    raw: RawExecution,
    config: &OracleConfig,
    workspace: &Path,
    redact: impl Fn(&str) -> String,
) -> Result<RunEvidence> {
    let stdout_raw = String::from_utf8_lossy(&raw.stdout);
    let stderr_raw = String::from_utf8_lossy(&raw.stderr);
    let mut checks = Vec::new();

    let exit_accepted = raw
        .exit_code
        .is_some_and(|code| config.accepted_exit_codes.contains(&code))
        && !raw.timed_out
        && !raw.interrupted;
    checks.push(OracleCheck {
        check: "exit_code".into(),
        accepted: exit_accepted,
        detail: if raw.timed_out {
            "command timed out".into()
        } else if raw.interrupted {
            "command was interrupted".into()
        } else {
            format!(
                "actual {:?}; accepted {:?}",
                raw.exit_code, config.accepted_exit_codes
            )
        },
    });

    for needle in &config.stdout_contains {
        checks.push(OracleCheck {
            check: "stdout_contains".into(),
            accepted: stdout_raw.contains(needle),
            detail: format!("required text `{needle}`"),
        });
    }
    for needle in &config.stderr_not_contains {
        checks.push(OracleCheck {
            check: "stderr_not_contains".into(),
            accepted: !stderr_raw.contains(needle),
            detail: format!("forbidden text `{needle}`"),
        });
    }
    if let Some(pattern) = &config.stdout_regex {
        let regex = Regex::new(pattern).context("validated stdout regex became invalid")?;
        checks.push(OracleCheck {
            check: "stdout_regex".into(),
            accepted: regex.is_match(&stdout_raw),
            detail: format!("required pattern `{pattern}`"),
        });
    }
    for path in &config.required_files {
        let safe = safe_join(workspace, path)?;
        checks.push(OracleCheck {
            check: "required_file".into(),
            accepted: safe.is_file(),
            detail: format!("required `{}`", path.display()),
        });
    }
    for path in &config.forbidden_files {
        let safe = safe_join(workspace, path)?;
        checks.push(OracleCheck {
            check: "forbidden_file".into(),
            accepted: !safe.exists(),
            detail: format!("forbidden `{}`", path.display()),
        });
    }

    Ok(RunEvidence {
        accepted: checks.iter().all(|check| check.accepted),
        exit_code: raw.exit_code,
        duration_ms: raw.duration_ms,
        timed_out: raw.timed_out,
        interrupted: raw.interrupted,
        output_truncated: raw.output_truncated,
        stdout_summary: redact(&stdout_raw),
        stderr_summary: redact(&stderr_raw),
        oracle_checks: checks,
    })
}

fn safe_join(root: &Path, relative: &Path) -> Result<std::path::PathBuf> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        anyhow::bail!(
            "oracle file path `{}` must remain inside the copied workspace",
            relative.display()
        );
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(code: i32, stdout: &[u8]) -> RawExecution {
        RawExecution {
            exit_code: Some(code),
            duration_ms: 1,
            timed_out: false,
            interrupted: false,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
            output_truncated: false,
        }
    }

    #[test]
    fn exit_and_text_oracle() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut config = OracleConfig::default();
        config.stdout_contains.push("passed".into());
        let result = evaluate(
            raw(0, b"all passed"),
            &config,
            directory.path(),
            str::to_owned,
        )
        .expect("oracle");
        assert!(result.accepted);
    }

    #[test]
    fn traversal_in_file_oracle_is_rejected() {
        assert!(safe_join(Path::new("/tmp/project"), Path::new("../secret")).is_err());
    }
}
