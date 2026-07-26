use crate::config::{WorkspaceConfig, WorkspaceMode};
use crate::platform;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use walkdir::WalkDir;

#[derive(Debug)]
pub struct IsolatedWorkspace {
    root: TempDir,
    project: PathBuf,
}

impl IsolatedWorkspace {
    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn temporary_root(&self) -> &Path {
        self.root.path()
    }
}

pub fn create(
    source: &Path,
    config: &WorkspaceConfig,
    requested_name: &str,
) -> Result<IsolatedWorkspace> {
    let root = tempfile::Builder::new()
        .prefix("assumezero-")
        .tempdir()
        .context("could not create an isolated temporary directory")?;
    let project = root.path().join(requested_name);
    fs::create_dir_all(&project)?;
    match config.mode {
        WorkspaceMode::WorkingTree => copy_working_tree(source, &project, config)?,
        WorkspaceMode::GitClean => copy_git_clean(source, &project, config)?,
    }
    Ok(IsolatedWorkspace { root, project })
}

fn is_excluded(relative: &Path, excludes: &[String]) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    excludes.iter().any(|exclude| {
        let exclude = exclude.trim_matches('/');
        normalized == exclude || normalized.starts_with(&format!("{exclude}/"))
    })
}

fn copy_working_tree(source: &Path, destination: &Path, config: &WorkspaceConfig) -> Result<()> {
    let mut copied_bytes = 0_u64;
    let limit = config.max_size_mib.saturating_mul(1_048_576);
    let walker = WalkDir::new(source).follow_links(false).into_iter();
    for entry in walker.filter_entry(|entry| {
        entry
            .path()
            .strip_prefix(source)
            .map_or(true, |relative| !is_excluded(relative, &config.exclude))
    }) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if is_excluded(relative, &config.exclude) {
            continue;
        }
        copy_entry(
            source,
            entry.path(),
            &destination.join(relative),
            config,
            &mut copied_bytes,
            limit,
        )?;
    }
    Ok(())
}

fn copy_git_clean(source: &Path, destination: &Path, config: &WorkspaceConfig) -> Result<()> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(source)
        .output()
        .context("git-clean mode requires Git")?;
    if !output.status.success() {
        bail!("git-clean mode requires the source directory to be a Git repository");
    }
    let mut copied_bytes = 0_u64;
    let limit = config.max_size_mib.saturating_mul(1_048_576);
    let mut paths: Vec<PathBuf> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|bytes| PathBuf::from(String::from_utf8_lossy(bytes).into_owned()))
        .collect();
    paths.extend(config.include_untracked.iter().cloned());
    paths.sort();
    paths.dedup();
    for relative in paths {
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || is_excluded(&relative, &config.exclude)
        {
            continue;
        }
        let from = source.join(&relative);
        let to = destination.join(&relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_entry(source, &from, &to, config, &mut copied_bytes, limit)?;
    }
    Ok(())
}

fn copy_entry(
    source_root: &Path,
    from: &Path,
    to: &Path,
    config: &WorkspaceConfig,
    copied_bytes: &mut u64,
    limit: u64,
) -> Result<()> {
    let metadata = fs::symlink_metadata(from)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(from)?;
        let lexical_target = if target.is_absolute() {
            platform::normalize_path(&target)
        } else {
            platform::normalize_path(&from.parent().unwrap_or(source_root).join(&target))
        };
        if !config.allow_external_symlinks && !lexical_target.starts_with(source_root) {
            bail!(
                "external symlink `{}` was not copied; its target was not read. \
                 Remove it, exclude it, or explicitly set `workspace.allow_external_symlinks = true` after reviewing the risk",
                from.strip_prefix(source_root).unwrap_or(from).display()
            );
        }
        create_symlink(&target, to, lexical_target.is_dir())?;
    } else if metadata.is_dir() {
        fs::create_dir_all(to)?;
    } else if metadata.is_file() {
        *copied_bytes = copied_bytes.saturating_add(metadata.len());
        if *copied_bytes > limit {
            bail!(
                "workspace copy exceeded the configured limit of {} MiB; \
                 add exclusions or increase `workspace.max_size_mib`",
                config.max_size_mib
            );
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        fs::set_permissions(to, metadata.permissions())?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path, _is_dir: bool) -> Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path, is_dir: bool) -> Result<()> {
    if is_dir {
        std::os::windows::fs::symlink_dir(target, link)?;
    } else {
        std::os::windows::fs::symlink_file(target, link)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_does_not_hardlink_source_files() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("file"), "original").expect("write");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        fs::write(copy.project().join("file"), "changed").expect("write copy");
        assert_eq!(
            fs::read_to_string(source.path().join("file")).expect("read"),
            "original"
        );
    }

    #[test]
    fn excludes_are_not_copied() {
        let source = tempfile::tempdir().expect("source");
        fs::create_dir(source.path().join(".git")).expect("mkdir");
        fs::write(source.path().join(".git/config"), "secret").expect("write");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        assert!(!copy.project().join(".git").exists());
    }

    #[test]
    fn git_clean_copies_tracked_and_explicitly_allowed_untracked_files() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("tracked.txt"), "tracked").expect("tracked");
        fs::write(source.path().join("allowed.txt"), "allowed").expect("allowed");
        fs::write(source.path().join("other.txt"), "other").expect("other");
        let init = Command::new("git")
            .arg("init")
            .current_dir(source.path())
            .output()
            .expect("git init");
        assert!(init.status.success());
        let add = Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(source.path())
            .output()
            .expect("git add");
        assert!(add.status.success());

        let mut config = WorkspaceConfig {
            mode: WorkspaceMode::GitClean,
            ..WorkspaceConfig::default()
        };
        config.include_untracked.push(PathBuf::from("allowed.txt"));
        let copy = create(source.path(), &config, "project").expect("copy");
        assert!(copy.project().join("tracked.txt").is_file());
        assert!(copy.project().join("allowed.txt").is_file());
        assert!(!copy.project().join("other.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn external_symlinks_are_refused_without_reading_target() {
        let source = tempfile::tempdir().expect("source");
        std::os::unix::fs::symlink("/definitely/not/read", source.path().join("outside"))
            .expect("symlink");
        let result = create(source.path(), &WorkspaceConfig::default(), "project");
        assert!(result.is_err());
    }
}
