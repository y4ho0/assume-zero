use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use walkdir::WalkDir;

const MAX_FINGERPRINT_PATH_BYTES: usize = 64 * 1_048_576;
const MAX_GIT_STATUS_BYTES: u64 = 64 * 1_048_576;

pub fn source_fingerprint(root: &Path, max_entries: usize) -> Result<String> {
    let root = root
        .canonicalize()
        .context("source root could not be resolved for fingerprinting")?;
    let mut entries = Vec::new();
    let mut path_bytes = 0_usize;
    let walker = WalkDir::new(&root).follow_links(false).into_iter();
    for entry in walker.filter_entry(|entry| {
        let Ok(relative) = entry.path().strip_prefix(&root) else {
            return false;
        };
        let first = relative.components().next().map(|value| value.as_os_str());
        first != Some(std::ffi::OsStr::new(".git"))
            && first != Some(std::ffi::OsStr::new(".assumezero"))
    }) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(&root)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if entries.len() >= max_entries {
            anyhow::bail!(
                "source fingerprint would exceed the configured limit of {max_entries} entries"
            );
        }
        path_bytes = path_bytes
            .checked_add(relative.as_os_str().as_encoded_bytes().len())
            .context("source fingerprint path storage overflowed")?;
        if path_bytes > MAX_FINGERPRINT_PATH_BYTES {
            anyhow::bail!(
                "source fingerprint paths would exceed the {} MiB safety limit",
                MAX_FINGERPRINT_PATH_BYTES / 1_048_576
            );
        }
        entries.push(relative.to_path_buf());
    }
    entries.sort();
    let mut hasher = Sha256::new();
    for relative in entries {
        let path = root.join(&relative);
        update_os_str(&mut hasher, b"path", relative.as_os_str());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            update_frame(&mut hasher, b"type", b"symlink");
            let target = fs::read_link(&path)?;
            update_os_str(&mut hasher, b"target", target.as_os_str());
        } else if metadata.is_file() {
            update_frame(&mut hasher, b"type", b"file");
            hasher.update(metadata.len().to_le_bytes());
            let resolved = path.canonicalize().with_context(|| {
                format!(
                    "could not resolve `{}` for fingerprinting",
                    relative.display()
                )
            })?;
            if resolved.strip_prefix(&root).is_err() {
                anyhow::bail!(
                    "could not fingerprint `{}` because it resolved outside the source root",
                    relative.display()
                );
            }
            let mut file = File::open(&resolved)
                .with_context(|| format!("could not fingerprint `{}`", relative.display()))?;
            let mut buffer = [0_u8; 65_536];
            let mut total = 0_u64;
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total = total
                    .checked_add(count as u64)
                    .context("fingerprinted file size overflowed")?;
                hasher.update(&buffer[..count]);
            }
            if total != metadata.len() {
                anyhow::bail!(
                    "could not fingerprint `{}` because it changed while being read",
                    relative.display()
                );
            }
        } else if metadata.is_dir() {
            update_frame(&mut hasher, b"type", b"dir");
        } else {
            update_frame(&mut hasher, b"type", b"special");
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn update_frame(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label);
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(unix)]
fn update_os_str(hasher: &mut Sha256, label: &[u8], value: &OsStr) {
    use std::os::unix::ffi::OsStrExt;
    update_frame(hasher, label, value.as_bytes());
}

#[cfg(windows)]
fn update_os_str(hasher: &mut Sha256, label: &[u8], value: &OsStr) {
    use std::os::windows::ffi::OsStrExt;
    let encoded: Vec<u8> = value.encode_wide().flat_map(u16::to_le_bytes).collect();
    update_frame(hasher, label, &encoded);
}

pub fn git_status(root: &Path) -> Option<String> {
    let mut child = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    let mut total = 0_u64;
    loop {
        let count = match stdout.read(&mut buffer) {
            Ok(count) => count,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        };
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64)?;
        if total > MAX_GIT_STATUS_BYTES {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        hasher.update(&buffer[..count]);
    }
    child
        .wait()
        .ok()?
        .success()
        .then(|| format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_change_changes_fingerprint() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("file"), "one").expect("write");
        let first = source_fingerprint(directory.path(), 100).expect("fingerprint");
        fs::write(directory.path().join("file"), "two").expect("write");
        let second = source_fingerprint(directory.path(), 100).expect("fingerprint");
        assert_ne!(first, second);
    }

    #[test]
    fn report_directory_is_ignored() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("file"), "one").expect("write");
        let first = source_fingerprint(directory.path(), 100).expect("fingerprint");
        fs::create_dir(directory.path().join(".assumezero")).expect("mkdir");
        fs::write(directory.path().join(".assumezero/report"), "metadata").expect("write");
        assert_eq!(
            first,
            source_fingerprint(directory.path(), 100).expect("fingerprint")
        );
    }

    #[test]
    fn fingerprint_entry_collection_is_bounded() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("one"), "one").expect("one");
        fs::write(directory.path().join("two"), "two").expect("two");
        assert!(source_fingerprint(directory.path(), 1).is_err());
    }
}
