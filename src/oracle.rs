use crate::config::OracleConfig;
use crate::model::{OracleCheck, RawExecution, RunEvidence};
use crate::platform;
use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::io::ErrorKind;
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
    for check in &mut checks {
        check.detail = redact(&check.detail);
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
    if !platform::is_portable_relative_path(relative) {
        anyhow::bail!(
            "oracle file path `{}` must remain inside the copied workspace",
            relative.display()
        );
    }
    let root_metadata = fs::symlink_metadata(root)
        .context("copied workspace root could not be inspected for a file oracle")?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        anyhow::bail!("copied workspace root was replaced by a symlink or non-directory entry");
    }
    let canonical_root = root
        .canonicalize()
        .context("copied workspace root could not be resolved for a file oracle")?;
    if canonical_root != root {
        anyhow::bail!("copied workspace root resolved differently before a file oracle");
    }
    let candidate = canonical_root.join(relative);
    let mut current = canonical_root.clone();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                let resolved = current.canonicalize().with_context(|| {
                    format!(
                        "oracle file path `{}` contains a broken or unresolvable symlink",
                        relative.display()
                    )
                })?;
                if resolved.strip_prefix(&canonical_root).is_err() {
                    anyhow::bail!(
                        "oracle file path `{}` resolves outside the copied workspace",
                        relative.display()
                    );
                }
                current = resolved;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(candidate),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "oracle file path `{}` could not be inspected",
                        relative.display()
                    )
                });
            }
        }
    }
    Ok(current)
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

    #[test]
    fn windows_style_traversal_is_rejected_portably() {
        let directory = tempfile::tempdir().expect("tempdir");
        for path in ["C:secret", "C:\\secret"] {
            assert!(
                safe_join(directory.path(), Path::new(path)).is_err(),
                "{path}"
            );
        }
        #[cfg(not(windows))]
        assert!(safe_join(directory.path(), Path::new("nested\\secret")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn file_oracle_refuses_symlinks_outside_workspace() {
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "outside").expect("outside file");
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("link")).expect("symlink");
        assert!(safe_join(
            &workspace.path().canonicalize().expect("workspace"),
            Path::new("link/secret")
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn file_oracle_refuses_a_replaced_workspace_root() {
        let parent = tempfile::tempdir().expect("parent");
        let workspace = parent.path().join("workspace");
        let moved = parent.path().join("moved");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir(&workspace).expect("workspace");
        fs::write(outside.path().join("secret"), "outside").expect("outside file");
        let canonical_workspace = workspace.canonicalize().expect("canonical workspace");
        fs::rename(&workspace, moved).expect("move workspace");
        std::os::unix::fs::symlink(outside.path(), &workspace).expect("replace with symlink");
        assert!(safe_join(&canonical_workspace, Path::new("secret")).is_err());
    }
}
