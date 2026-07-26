use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

pub fn source_fingerprint(root: &Path) -> Result<String> {
    let mut entries = Vec::new();
    let walker = WalkDir::new(root).follow_links(false).into_iter();
    for entry in walker.filter_entry(|entry| {
        let Ok(relative) = entry.path().strip_prefix(root) else {
            return false;
        };
        let first = relative.components().next().map(|value| value.as_os_str());
        first != Some(std::ffi::OsStr::new(".git"))
            && first != Some(std::ffi::OsStr::new(".assumezero"))
    }) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(root)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        entries.push(relative.to_path_buf());
    }
    entries.sort();
    let mut hasher = Sha256::new();
    for relative in entries {
        let path = root.join(&relative);
        hasher.update(relative.to_string_lossy().as_bytes());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            hasher.update(b"symlink:");
            hasher.update(fs::read_link(&path)?.to_string_lossy().as_bytes());
        } else if metadata.is_file() {
            hasher.update(b"file:");
            let bytes = fs::read(&path)
                .with_context(|| format!("could not fingerprint `{}`", relative.display()))?;
            hasher.update(&bytes);
        } else if metadata.is_dir() {
            hasher.update(b"dir:");
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn git_status(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_change_changes_fingerprint() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("file"), "one").expect("write");
        let first = source_fingerprint(directory.path()).expect("fingerprint");
        fs::write(directory.path().join("file"), "two").expect("write");
        let second = source_fingerprint(directory.path()).expect("fingerprint");
        assert_ne!(first, second);
    }

    #[test]
    fn report_directory_is_ignored() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("file"), "one").expect("write");
        let first = source_fingerprint(directory.path()).expect("fingerprint");
        fs::create_dir(directory.path().join(".assumezero")).expect("mkdir");
        fs::write(directory.path().join(".assumezero/report"), "metadata").expect("write");
        assert_eq!(
            first,
            source_fingerprint(directory.path()).expect("fingerprint")
        );
    }
}
