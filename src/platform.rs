use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn facts() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("os".into(), env::consts::OS.into()),
        ("arch".into(), env::consts::ARCH.into()),
        ("family".into(), env::consts::FAMILY.into()),
    ])
}

pub fn environment_name_eq(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        left.eq_ignore_ascii_case(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

pub fn environment_value<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a String> {
    environment
        .iter()
        .find(|(candidate, _)| environment_name_eq(candidate, name))
        .map(|(_, value)| value)
}

pub fn contains_environment_name(environment: &BTreeMap<String, String>, name: &str) -> bool {
    environment
        .keys()
        .any(|candidate| environment_name_eq(candidate, name))
}

pub fn set_environment_value(
    environment: &mut BTreeMap<String, String>,
    name: &str,
    value: String,
) {
    let existing = environment
        .keys()
        .find(|candidate| environment_name_eq(candidate, name))
        .cloned();
    if let Some(existing) = existing {
        environment.remove(&existing);
    }
    environment.insert(name.into(), value);
}

pub fn name_in_list(names: &[String], candidate: &str) -> bool {
    names
        .iter()
        .any(|name| environment_name_eq(name, candidate))
}

pub fn necessary_environment() -> BTreeSet<String> {
    #[cfg(windows)]
    {
        ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"]
            .into_iter()
            .map(String::from)
            .collect()
    }
    #[cfg(not(windows))]
    {
        ["PATH", "TERM", "TMPDIR"]
            .into_iter()
            .map(String::from)
            .collect()
    }
}

pub fn minimal_system_path() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut paths = Vec::new();
        if let Some(root) = env::var_os("SystemRoot") {
            let root = PathBuf::from(root);
            paths.push(root.join("System32"));
            paths.push(root);
        }
        paths
    }
    #[cfg(not(windows))]
    {
        ["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .into_iter()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .collect()
    }
}

pub fn split_path(value: &OsString) -> Vec<PathBuf> {
    env::split_paths(value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

pub fn join_path(paths: &[PathBuf]) -> Option<OsString> {
    env::join_paths(paths).ok()
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

pub fn deduplicate_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for path in paths {
        let normalized = normalize_path(&path);
        #[cfg(windows)]
        let key = normalized.to_string_lossy().to_lowercase();
        #[cfg(not(windows))]
        let key = normalized.to_string_lossy().to_string();
        if seen.insert(key) {
            result.push(normalized);
        }
    }
    result
}

pub fn resolve_program(name: &str, path_value: Option<&OsString>) -> Option<PathBuf> {
    let requested = Path::new(name);
    if requested.components().count() > 1 || requested.is_absolute() {
        return requested.is_file().then(|| requested.to_path_buf());
    }
    let paths = path_value.map_or_else(Vec::new, split_path);
    #[cfg(windows)]
    {
        let extensions: Vec<String> = env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|value| value.to_ascii_lowercase())
            .collect();
        for directory in paths {
            let plain = directory.join(name);
            if plain.is_file() {
                return Some(plain);
            }
            if Path::new(name).extension().is_none() {
                for extension in &extensions {
                    let candidate = directory.join(format!("{name}{extension}"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        for directory in paths {
            let candidate = directory.join(name);
            if candidate.is_file()
                && candidate
                    .metadata()
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn redacted_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home {
        if let Ok(relative) = path.strip_prefix(home) {
            return if relative.as_os_str().is_empty() {
                "<HOME>".into()
            } else {
                format!("<HOME>/{}", relative.to_string_lossy().replace('\\', "/"))
            };
        }
    }
    let system = minimal_system_path();
    if system.iter().any(|base| path.starts_with(base)) {
        return "<SYSTEM>".into();
    }
    path.to_string_lossy().replace('\\', "/")
}

pub fn command_for_program(executable: &Path) -> std::process::Command {
    #[cfg(windows)]
    {
        let extension = executable
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "cmd" || extension == "bat" {
            let mut command = std::process::Command::new(
                env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe")),
            );
            command.arg("/D").arg("/S").arg("/C").arg(executable);
            return command;
        }
    }
    std::process::Command::new(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_deduplication_preserves_order() {
        let result = deduplicate_paths([
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/a"),
        ]);
        assert_eq!(result, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn lexical_normalization_removes_parent_components() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_path_split_uses_colons() {
        assert_eq!(
            split_path(&OsString::from("/a:/b")),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn windows_style_path_can_be_tested_without_host_separator() {
        let parts: Vec<_> = "C:\\one;D:\\two".split(';').collect();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn environment_name_comparison_matches_platform_rules() {
        assert!(environment_name_eq("PATH", "PATH"));
        #[cfg(windows)]
        assert!(environment_name_eq("Path", "PATH"));
        #[cfg(not(windows))]
        assert!(!environment_name_eq("Path", "PATH"));
    }

    #[cfg(windows)]
    #[test]
    fn setting_windows_environment_replaces_differently_cased_key() {
        let mut environment = BTreeMap::from([("Path".into(), "original".into())]);
        set_environment_value(&mut environment, "PATH", "minimal".into());
        assert_eq!(environment.len(), 1);
        assert_eq!(
            environment_value(&environment, "Path").map(String::as_str),
            Some("minimal")
        );
    }
}
