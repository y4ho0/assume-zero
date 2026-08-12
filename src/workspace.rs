use crate::config::{WorkspaceConfig, WorkspaceMode};
use crate::platform;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
#[cfg(unix)]
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;
use tempfile::TempDir;
use walkdir::WalkDir;

pub struct IsolatedWorkspace {
    _root: TempDir,
    temporary_root: PathBuf,
    project: PathBuf,
}

impl IsolatedWorkspace {
    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn temporary_root(&self) -> &Path {
        &self.temporary_root
    }

    pub fn validate_boundary(&self) -> Result<()> {
        let temporary_metadata = fs::symlink_metadata(&self.temporary_root)
            .context("isolated temporary root could not be inspected after command execution")?;
        if temporary_metadata.file_type().is_symlink() || !temporary_metadata.is_dir() {
            bail!("isolated temporary root was replaced by a symlink or non-directory entry");
        }
        let resolved_temporary = self
            .temporary_root
            .canonicalize()
            .context("isolated temporary root could not be resolved after command execution")?;
        if resolved_temporary != self.temporary_root {
            bail!("isolated temporary root resolved differently after command execution");
        }

        let project_metadata = fs::symlink_metadata(&self.project)
            .context("isolated project root could not be inspected after command execution")?;
        if project_metadata.file_type().is_symlink() || !project_metadata.is_dir() {
            bail!("isolated project root was replaced by a symlink or non-directory entry");
        }
        let resolved_project = self
            .project
            .canonicalize()
            .context("isolated project root could not be resolved after command execution")?;
        if resolved_project != self.project {
            bail!("isolated project root resolved differently after command execution");
        }
        ensure_contained(
            &self.temporary_root,
            &resolved_project,
            "isolated project root",
        )
    }
}

#[derive(Clone)]
enum PlannedKind {
    Directory,
    File {
        len: u64,
        modified: Option<SystemTime>,
    },
    Symlink {
        source_target: PathBuf,
        destination_target: PathBuf,
        target_is_dir: bool,
    },
}

#[derive(Clone)]
struct PlannedEntry {
    relative: PathBuf,
    source: PathBuf,
    resolved_source: Option<PathBuf>,
    kind: PlannedKind,
}

struct CopyPlan {
    entries: Vec<PlannedEntry>,
}

struct PlanBudget {
    bytes: u64,
    entries: usize,
    byte_limit: u64,
    entry_limit: usize,
}

const MAX_PATH_STORAGE_BYTES: usize = 64 * 1_048_576;

fn add_path_storage(total: &mut usize, path: &Path, label: &str) -> Result<()> {
    *total = total
        .checked_add(path.as_os_str().as_encoded_bytes().len())
        .context("workspace path storage count overflowed")?;
    if *total > MAX_PATH_STORAGE_BYTES {
        bail!(
            "{label} would exceed the {} MiB path-storage safety limit",
            MAX_PATH_STORAGE_BYTES / 1_048_576
        );
    }
    Ok(())
}

impl PlanBudget {
    fn new(config: &WorkspaceConfig) -> Result<Self> {
        let byte_limit = config
            .max_size_mib
            .checked_mul(1_048_576)
            .context("workspace.max_size_mib is too large to represent safely")?;
        Ok(Self {
            bytes: 0,
            entries: 0,
            byte_limit,
            entry_limit: config.max_entries,
        })
    }

    fn add(&mut self, kind: &PlannedKind, config: &WorkspaceConfig) -> Result<()> {
        self.entries = self
            .entries
            .checked_add(1)
            .context("workspace entry count overflowed")?;
        if self.entries > self.entry_limit {
            bail!(
                "workspace copy would exceed the configured limit of {} entries; add exclusions or increase `workspace.max_entries`",
                config.max_entries
            );
        }
        if let PlannedKind::File { len, .. } = kind {
            self.bytes = self
                .bytes
                .checked_add(*len)
                .context("workspace byte count overflowed")?;
            if self.bytes > self.byte_limit {
                bail!(
                    "workspace copy would exceed the configured limit of {} MiB; add exclusions or increase `workspace.max_size_mib`",
                    config.max_size_mib
                );
            }
        }
        Ok(())
    }
}

pub fn create(
    source: &Path,
    config: &WorkspaceConfig,
    requested_name: &str,
) -> Result<IsolatedWorkspace> {
    validate_workspace_name(requested_name)?;
    let source_root = source.canonicalize().with_context(|| {
        format!(
            "repository root `{}` could not be resolved",
            source.display()
        )
    })?;
    let plan = match config.mode {
        WorkspaceMode::WorkingTree => plan_working_tree(&source_root, config)?,
        WorkspaceMode::GitClean => plan_git_clean(&source_root, config)?,
    };

    let root = tempfile::Builder::new()
        .prefix("assumezero-")
        .tempdir()
        .context("could not create an isolated temporary directory")?;
    let temporary_root = root
        .path()
        .canonicalize()
        .context("isolated temporary directory could not be resolved")?;
    let project_path = temporary_root.join(requested_name);
    populate(&source_root, &temporary_root, &project_path, &plan)?;
    let project = project_path
        .canonicalize()
        .context("isolated project directory could not be resolved after population")?;
    let isolated = IsolatedWorkspace {
        _root: root,
        temporary_root,
        project,
    };
    isolated.validate_boundary()?;
    Ok(isolated)
}

fn validate_workspace_name(requested_name: &str) -> Result<()> {
    if !platform::is_single_normal_component(requested_name) {
        bail!(
            "workspace name `{requested_name}` must be one normal path component without separators, roots, drive prefixes, `.` or `..`"
        );
    }
    Ok(())
}

fn is_excluded(relative: &Path, excludes: &[String]) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    excludes.iter().any(|exclude| {
        let exclude = exclude.trim_matches('/');
        normalized == exclude || normalized.starts_with(&format!("{exclude}/"))
    })
}

fn plan_working_tree(source_root: &Path, config: &WorkspaceConfig) -> Result<CopyPlan> {
    let mut budget = PlanBudget::new(config)?;
    let mut entries = Vec::new();
    let walker = WalkDir::new(source_root).follow_links(false).into_iter();
    for entry in walker.filter_entry(|entry| {
        entry
            .path()
            .strip_prefix(source_root)
            .map_or(true, |relative| !is_excluded(relative, &config.exclude))
    }) {
        let entry = entry.context("workspace source could not be traversed")?;
        let relative = entry.path().strip_prefix(source_root)?;
        if relative.as_os_str().is_empty() || is_excluded(relative, &config.exclude) {
            continue;
        }
        let planned = plan_entry(source_root, relative, config)?;
        budget.add(&planned.kind, config)?;
        entries.push(planned);
    }
    Ok(CopyPlan { entries })
}

fn plan_git_clean(source_root: &Path, config: &WorkspaceConfig) -> Result<CopyPlan> {
    let mut child = Command::new("git")
        .args(["ls-files", "-z", "--cached", "--"])
        .current_dir(source_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("git-clean mode requires Git")?;
    let mut with_parents = BTreeSet::new();
    let mut path_storage_bytes = 0_usize;
    let mut git_stream_bytes = 0_usize;
    let parse_result = (|| -> Result<()> {
        let stdout = child
            .stdout
            .take()
            .context("Git tracked-file output was unavailable")?;
        let mut reader = BufReader::new(stdout);
        let mut bytes = Vec::new();
        while read_nul_path(&mut reader, &mut bytes)? {
            if bytes.is_empty() {
                continue;
            }
            git_stream_bytes = git_stream_bytes
                .checked_add(bytes.len())
                .context("Git path stream byte count overflowed")?;
            if git_stream_bytes > MAX_PATH_STORAGE_BYTES {
                bail!(
                    "Git path stream would exceed the {} MiB safety limit",
                    MAX_PATH_STORAGE_BYTES / 1_048_576
                );
            }
            let relative = parse_git_path(&bytes)?;
            insert_requested_path(&mut with_parents, &mut path_storage_bytes, relative, config)?;
        }
        Ok(())
    })();
    if let Err(error) = parse_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    if !child.wait()?.success() {
        bail!("git-clean mode requires the source directory to be a Git repository");
    }
    for relative in &config.include_untracked {
        insert_requested_path(
            &mut with_parents,
            &mut path_storage_bytes,
            relative.clone(),
            config,
        )?;
    }

    let mut budget = PlanBudget::new(config)?;
    let mut entries = Vec::new();
    for relative in with_parents {
        let planned = plan_entry(source_root, &relative, config)?;
        budget.add(&planned.kind, config)?;
        entries.push(planned);
    }
    Ok(CopyPlan { entries })
}

const MAX_GIT_PATH_BYTES: usize = 1_048_576;

fn read_nul_path(reader: &mut impl BufRead, output: &mut Vec<u8>) -> Result<bool> {
    output.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if output.is_empty() {
                return Ok(false);
            }
            bail!("Git tracked-file output ended without a NUL path terminator");
        }
        let (chunk, consumed, complete) = match available.iter().position(|byte| *byte == 0) {
            Some(position) => (&available[..position], position + 1, true),
            None => (available, available.len(), false),
        };
        if output.len().saturating_add(chunk.len()) > MAX_GIT_PATH_BYTES {
            bail!(
                "Git returned a tracked path longer than the {} byte safety limit",
                MAX_GIT_PATH_BYTES
            );
        }
        output.extend_from_slice(chunk);
        reader.consume(consumed);
        if complete {
            return Ok(true);
        }
    }
}

#[cfg(unix)]
fn parse_git_path(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(windows)]
fn parse_git_path(bytes: &[u8]) -> Result<PathBuf> {
    let value = std::str::from_utf8(bytes)
        .context("Git returned a path that is not valid UTF-8 on Windows")?;
    Ok(PathBuf::from(value))
}

fn insert_requested_path(
    with_parents: &mut BTreeSet<PathBuf>,
    path_storage_bytes: &mut usize,
    relative: PathBuf,
    config: &WorkspaceConfig,
) -> Result<()> {
    validate_copy_relative(&relative)?;
    if is_excluded(&relative, &config.exclude) {
        return Ok(());
    }
    let mut current = relative.as_path();
    while !current.as_os_str().is_empty() {
        if !with_parents.contains(current) {
            add_path_storage(path_storage_bytes, current, "Git path collection")?;
            with_parents.insert(current.to_path_buf());
            if with_parents.len() > config.max_entries {
                bail!(
                    "workspace copy would exceed the configured limit of {} entries while reading Git paths",
                    config.max_entries
                );
            }
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
    Ok(())
}

fn validate_copy_relative(relative: &Path) -> Result<()> {
    if !platform::is_portable_relative_path(relative) {
        bail!(
            "workspace source path `{}` is not a safe relative path",
            relative.display()
        );
    }
    Ok(())
}

fn ensure_no_symlink_ancestor(source_root: &Path, relative: &Path) -> Result<()> {
    let components: Vec<_> = relative.components().collect();
    let mut current = source_root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).with_context(|| {
            format!(
                "workspace source ancestor `{}` could not be inspected",
                current.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                "workspace source path `{}` crosses symlink ancestor `{}`; nested paths through symlinks are refused",
                relative.display(),
                current
                    .strip_prefix(source_root)
                    .unwrap_or(&current)
                    .display()
            );
        }
        if !metadata.is_dir() {
            bail!(
                "workspace source ancestor `{}` is not a directory",
                current.display()
            );
        }
        let resolved = current.canonicalize()?;
        ensure_contained(source_root, &resolved, "workspace source ancestor")?;
    }
    Ok(())
}

fn plan_entry(
    source_root: &Path,
    relative: &Path,
    config: &WorkspaceConfig,
) -> Result<PlannedEntry> {
    validate_copy_relative(relative)?;
    ensure_no_symlink_ancestor(source_root, relative)?;
    let source = source_root.join(relative);
    let metadata = fs::symlink_metadata(&source).with_context(|| {
        format!(
            "workspace source `{}` could not be inspected",
            relative.display()
        )
    })?;

    if metadata.file_type().is_symlink() {
        let source_target = fs::read_link(&source)?;
        let target_candidate = if source_target.is_absolute() {
            source_target.clone()
        } else {
            source.parent().unwrap_or(source_root).join(&source_target)
        };
        let resolved_target = target_candidate.canonicalize().with_context(|| {
            format!(
                "symlink `{}` has a broken or unresolvable target and was refused",
                relative.display()
            )
        })?;
        let (destination_target, target_is_dir) = if let Ok(target_relative) =
            resolved_target.strip_prefix(source_root)
        {
            let link_parent = relative.parent().unwrap_or_else(|| Path::new(""));
            (
                relative_path(link_parent, target_relative),
                resolved_target.is_dir(),
            )
        } else if config.allow_external_symlinks {
            (source_target.clone(), resolved_target.is_dir())
        } else {
            bail!(
                    "external symlink `{}` was not copied; its resolved target is outside the repository. Remove it, exclude it, or explicitly set `workspace.allow_external_symlinks = true` after reviewing the risk",
                    relative.display()
                );
        };
        return Ok(PlannedEntry {
            relative: relative.to_path_buf(),
            source,
            resolved_source: None,
            kind: PlannedKind::Symlink {
                source_target,
                destination_target,
                target_is_dir,
            },
        });
    }

    let resolved_source = source.canonicalize().with_context(|| {
        format!(
            "workspace source `{}` could not be resolved",
            relative.display()
        )
    })?;
    ensure_contained(source_root, &resolved_source, "workspace source")?;
    let kind = if metadata.is_dir() {
        PlannedKind::Directory
    } else if metadata.is_file() {
        PlannedKind::File {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    } else {
        bail!(
            "workspace source `{}` has an unsupported special file type",
            relative.display()
        );
    };
    Ok(PlannedEntry {
        relative: relative.to_path_buf(),
        source,
        resolved_source: Some(resolved_source),
        kind,
    })
}

fn relative_path(from_directory: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from_directory.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
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

fn populate(
    source_root: &Path,
    temporary_root: &Path,
    project: &Path,
    plan: &CopyPlan,
) -> Result<()> {
    ensure_contained(temporary_root, project, "workspace destination")?;
    fs::create_dir(project).context("could not create the isolated project directory")?;
    let project_root = project
        .canonicalize()
        .context("isolated project directory could not be resolved")?;
    ensure_contained(temporary_root, &project_root, "workspace destination")?;

    if let Err(error) = execute_plan(source_root, &project_root, plan) {
        if let Err(cleanup) = fs::remove_dir_all(&project_root) {
            return Err(error).context(format!(
                "workspace copy failed and partial workspace cleanup also failed: {cleanup}"
            ));
        }
        return Err(error);
    }
    Ok(())
}

fn execute_plan(source_root: &Path, project_root: &Path, plan: &CopyPlan) -> Result<()> {
    for entry in plan
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, PlannedKind::Directory))
    {
        let destination = destination_path(project_root, &entry.relative)?;
        fs::create_dir_all(&destination)?;
        let resolved = destination.canonicalize()?;
        ensure_contained(project_root, &resolved, "workspace destination")?;
    }
    for entry in plan
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, PlannedKind::File { .. }))
    {
        copy_planned_file(source_root, project_root, entry)?;
    }
    for entry in plan
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, PlannedKind::Symlink { .. }))
    {
        copy_planned_symlink(source_root, project_root, entry)?;
    }
    Ok(())
}

fn destination_path(project_root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_copy_relative(relative)?;
    let destination = project_root.join(relative);
    ensure_contained(project_root, &destination, "workspace destination")?;
    Ok(destination)
}

fn ensure_destination_parent(project_root: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().unwrap_or(project_root);
    fs::create_dir_all(parent)?;
    let resolved = parent.canonicalize()?;
    ensure_contained(project_root, &resolved, "workspace destination parent")
}

fn copy_planned_file(source_root: &Path, project_root: &Path, entry: &PlannedEntry) -> Result<()> {
    let PlannedKind::File { len, modified } = entry.kind else {
        bail!("internal workspace plan type mismatch");
    };
    ensure_no_symlink_ancestor(source_root, &entry.relative)?;
    let current_metadata = fs::symlink_metadata(&entry.source)?;
    if !current_metadata.is_file() || current_metadata.file_type().is_symlink() {
        bail!(
            "workspace source `{}` changed type after preflight",
            entry.relative.display()
        );
    }
    if current_metadata.len() != len
        || modified.is_some_and(|expected| current_metadata.modified().ok() != Some(expected))
    {
        bail!(
            "workspace source `{}` changed after preflight",
            entry.relative.display()
        );
    }
    let resolved = entry.source.canonicalize()?;
    let planned_resolved = entry
        .resolved_source
        .as_ref()
        .context("file plan did not contain a resolved source")?;
    if &resolved != planned_resolved {
        bail!(
            "workspace source `{}` resolved differently after preflight",
            entry.relative.display()
        );
    }
    ensure_contained(source_root, &resolved, "workspace source")?;
    let mut source_file = File::open(planned_resolved)?;
    let opened_metadata = source_file.metadata()?;
    if !opened_metadata.is_file() || opened_metadata.len() != len {
        bail!(
            "workspace source `{}` changed while it was opened",
            entry.relative.display()
        );
    }
    let resolved_after_open = planned_resolved.canonicalize()?;
    if resolved_after_open != *planned_resolved {
        bail!(
            "workspace source `{}` changed while it was opened",
            entry.relative.display()
        );
    }
    ensure_contained(source_root, &resolved_after_open, "workspace source")?;

    let destination = destination_path(project_root, &entry.relative)?;
    ensure_destination_parent(project_root, &destination)?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    let copied = io::copy(
        &mut source_file.by_ref().take(len.saturating_add(1)),
        &mut destination_file,
    )?;
    if copied != len {
        bail!(
            "workspace source `{}` changed size during copy",
            entry.relative.display()
        );
    }
    destination_file.set_permissions(opened_metadata.permissions())?;
    Ok(())
}

fn copy_planned_symlink(
    source_root: &Path,
    project_root: &Path,
    entry: &PlannedEntry,
) -> Result<()> {
    let PlannedKind::Symlink {
        ref source_target,
        ref destination_target,
        target_is_dir,
    } = entry.kind
    else {
        bail!("internal workspace plan type mismatch");
    };
    ensure_no_symlink_ancestor(source_root, &entry.relative)?;
    let metadata = fs::symlink_metadata(&entry.source)?;
    if !metadata.file_type().is_symlink() || fs::read_link(&entry.source)? != *source_target {
        bail!(
            "workspace symlink `{}` changed after preflight",
            entry.relative.display()
        );
    }
    let destination = destination_path(project_root, &entry.relative)?;
    ensure_destination_parent(project_root, &destination)?;
    create_symlink(destination_target, &destination, target_is_dir)
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
    fn normal_nested_files_are_copied() {
        let source = tempfile::tempdir().expect("source");
        fs::create_dir_all(source.path().join("a/b")).expect("mkdir");
        fs::write(source.path().join("a/b/file"), "inside").expect("write");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        assert_eq!(
            fs::read_to_string(copy.project().join("a/b/file")).expect("read"),
            "inside"
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
    fn workspace_name_must_be_one_portable_normal_component() {
        for valid in ["project", "project copy", "项目-Δ"] {
            assert!(validate_workspace_name(valid).is_ok(), "{valid}");
        }
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
            assert!(validate_workspace_name(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn budget_is_rejected_before_destination_mutation() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("small"), "small").expect("write");
        let large = File::create(source.path().join("large")).expect("large");
        large.set_len(1_048_577).expect("size");
        let config = WorkspaceConfig {
            max_size_mib: 1,
            ..WorkspaceConfig::default()
        };
        let result = plan_working_tree(
            &source.path().canonicalize().expect("canonical source"),
            &config,
        );
        assert!(result.is_err());

        let root = tempfile::tempdir().expect("destination root");
        assert!(!root.path().join("project").exists());
    }

    #[test]
    fn entry_budget_covers_zero_length_entries() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("one"), "").expect("one");
        fs::write(source.path().join("two"), "").expect("two");
        let config = WorkspaceConfig {
            max_entries: 1,
            ..WorkspaceConfig::default()
        };
        assert!(create(source.path(), &config, "project").is_err());
    }

    #[test]
    fn source_change_after_preflight_fails_and_cleans_project() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("file"), "before").expect("write");
        let source_root = source.path().canonicalize().expect("source root");
        let plan = plan_working_tree(&source_root, &WorkspaceConfig::default()).expect("plan");
        fs::write(source.path().join("file"), "after-with-a-different-size").expect("change");

        let temporary = tempfile::tempdir().expect("temporary root");
        let temporary_root = temporary.path().canonicalize().expect("temporary root");
        let project = temporary_root.join("project");
        assert!(populate(&source_root, &temporary_root, &project, &plan).is_err());
        assert!(!project.exists());
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
    fn internal_final_symlinks_are_rewritten_inside_the_copy() {
        let source = tempfile::tempdir().expect("source");
        fs::create_dir(source.path().join("dir")).expect("dir");
        fs::write(source.path().join("sibling"), "inside").expect("write");
        std::os::unix::fs::symlink("../sibling", source.path().join("dir/link")).expect("symlink");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        assert_eq!(
            fs::read_to_string(copy.project().join("dir/link")).expect("read"),
            "inside"
        );
        assert!(!fs::read_link(copy.project().join("dir/link"))
            .expect("target")
            .is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn absolute_internal_symlink_does_not_point_back_to_source() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("target"), "source").expect("write");
        std::os::unix::fs::symlink(
            source.path().join("target"),
            source.path().join("absolute-link"),
        )
        .expect("symlink");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        fs::write(copy.project().join("absolute-link"), "copy").expect("write through link");
        assert_eq!(
            fs::read_to_string(source.path().join("target")).expect("source"),
            "source"
        );
    }

    #[cfg(unix)]
    #[test]
    fn internal_parent_symlink_is_rewritten_inside_the_copy() {
        let source = tempfile::tempdir().expect("source");
        fs::create_dir(source.path().join("dir")).expect("dir");
        fs::write(source.path().join("target"), "source").expect("target");
        std::os::unix::fs::symlink("..", source.path().join("dir/up")).expect("symlink");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        fs::write(copy.project().join("dir/up/target"), "copy").expect("write copy");
        assert_eq!(
            fs::read_to_string(source.path().join("target")).expect("source target"),
            "source"
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_and_broken_final_symlinks_are_refused() {
        let source = tempfile::tempdir().expect("source");
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret"), "outside").expect("outside");
        std::os::unix::fs::symlink(
            outside.path().join("secret"),
            source.path().join("absolute"),
        )
        .expect("absolute link");
        assert!(create(source.path(), &WorkspaceConfig::default(), "project").is_err());

        fs::remove_file(source.path().join("absolute")).expect("remove");
        std::os::unix::fs::symlink("missing", source.path().join("broken")).expect("broken link");
        assert!(create(source.path(), &WorkspaceConfig::default(), "project").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn relative_external_final_symlink_is_refused() {
        let parent = tempfile::tempdir().expect("parent");
        let source = parent.path().join("repo");
        let outside = parent.path().join("outside");
        fs::create_dir(&source).expect("source");
        fs::create_dir(&outside).expect("outside");
        fs::write(outside.join("secret"), "outside").expect("outside file");
        std::os::unix::fs::symlink("../outside/secret", source.join("link"))
            .expect("relative external symlink");
        assert!(create(&source, &WorkspaceConfig::default(), "project").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn replaced_project_root_fails_boundary_validation() {
        let source = tempfile::tempdir().expect("source");
        let outside = tempfile::tempdir().expect("outside");
        fs::write(source.path().join("file"), "inside").expect("source file");
        let copy = create(source.path(), &WorkspaceConfig::default(), "project").expect("copy");
        let moved = copy.temporary_root().join("moved-project");
        fs::rename(copy.project(), &moved).expect("move project");
        std::os::unix::fs::symlink(outside.path(), copy.project()).expect("replace project");
        assert!(copy.validate_boundary().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_symlinks_never_copy_external_files() {
        let source = tempfile::tempdir().expect("source");
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret"), "outside sentinel").expect("outside");
        std::os::unix::fs::symlink(outside.path(), source.path().join("link")).expect("symlink");
        let init = Command::new("git")
            .arg("init")
            .current_dir(source.path())
            .output()
            .expect("git init");
        assert!(init.status.success());

        let mut config = WorkspaceConfig {
            mode: WorkspaceMode::GitClean,
            allow_external_symlinks: true,
            ..WorkspaceConfig::default()
        };
        config.include_untracked.push(PathBuf::from("link/secret"));
        let error = create(source.path(), &config, "project")
            .err()
            .expect("intermediate symlink must fail");
        assert!(error.to_string().contains("symlink ancestor"));
        assert!(!error.to_string().contains("outside sentinel"));
    }

    #[cfg(unix)]
    #[test]
    fn relative_intermediate_symlink_to_sibling_is_refused() {
        let parent = tempfile::tempdir().expect("parent");
        let source = parent.path().join("repo");
        let sibling = parent.path().join("repo2");
        fs::create_dir(&source).expect("source");
        fs::create_dir(&sibling).expect("sibling");
        fs::write(sibling.join("secret"), "outside").expect("secret");
        std::os::unix::fs::symlink("../repo2", source.join("link")).expect("symlink");
        let init = Command::new("git")
            .arg("init")
            .current_dir(&source)
            .output()
            .expect("git init");
        assert!(init.status.success());
        let mut config = WorkspaceConfig {
            mode: WorkspaceMode::GitClean,
            ..WorkspaceConfig::default()
        };
        config.include_untracked.push(PathBuf::from("link/secret"));
        assert!(create(&source, &config, "project").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn git_paths_preserve_non_utf8_names() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let invalid = parse_git_path(b"invalid-\xff").expect("invalid path");
        let replacement = parse_git_path(b"replacement-\xef\xbf\xbd").expect("replacement path");
        assert_eq!(invalid.as_os_str().as_bytes(), b"invalid-\xff");
        assert_eq!(
            replacement.as_os_str().as_bytes(),
            OsString::from_vec(b"replacement-\xef\xbf\xbd".to_vec())
                .as_os_str()
                .as_bytes()
        );
        assert_ne!(invalid, replacement);
    }

    #[test]
    fn streamed_git_paths_are_length_bounded_and_require_nul() {
        let mut complete = std::io::Cursor::new(b"tracked\0".to_vec());
        let mut path = Vec::new();
        assert!(read_nul_path(&mut complete, &mut path).expect("path"));
        assert_eq!(path, b"tracked");
        assert!(!read_nul_path(&mut complete, &mut path).expect("end"));

        let mut truncated = std::io::Cursor::new(b"tracked".to_vec());
        assert!(read_nul_path(&mut truncated, &mut path).is_err());

        let mut oversized = std::io::Cursor::new(vec![b'x'; MAX_GIT_PATH_BYTES + 1]);
        assert!(read_nul_path(&mut oversized, &mut path).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_external_symlink_is_refused_when_symlink_creation_is_available() {
        use std::io::ErrorKind;

        let source = tempfile::tempdir().expect("source");
        let outside = tempfile::tempdir().expect("outside");
        let target = outside.path().join("secret");
        fs::write(&target, "outside").expect("outside file");
        match std::os::windows::fs::symlink_file(&target, source.path().join("link")) {
            Ok(()) => {
                assert!(create(source.path(), &WorkspaceConfig::default(), "project").is_err());
            }
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                eprintln!(
                    "Windows symlink privilege is unavailable; portable path tests still ran"
                );
            }
            Err(error) => panic!("unexpected symlink creation error: {error}"),
        }
    }
}
