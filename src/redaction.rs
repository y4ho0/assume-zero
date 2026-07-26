use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SENSITIVE_NAME_PARTS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "ACCESS_KEY",
    "PRIVATE_KEY",
    "AUTH",
    "CREDENTIAL",
];

#[derive(Debug, Clone)]
pub struct Redactor {
    exact_values: Vec<String>,
    home: Option<PathBuf>,
    project: PathBuf,
    temporary_roots: Vec<PathBuf>,
    patterns: Vec<Regex>,
}

impl Redactor {
    pub fn new(environment: &BTreeMap<String, String>, project: &Path) -> Self {
        let exact_values = environment
            .iter()
            .filter(|(name, value)| is_sensitive_name(name) && value.len() >= 4)
            .map(|(_, value)| value.clone())
            .collect();
        let patterns = [
            r"(?i)Bearer\s+[A-Za-z0-9._~+/=-]{8,}",
            r"\bgh[pousr]_[A-Za-z0-9]{20,}\b",
            r"\bAKIA[0-9A-Z]{16}\b",
            r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
            r"(?i)\b(?:postgres|mysql|mongodb(?:\+srv)?)://[^\s]+",
        ]
        .into_iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .collect();
        Self {
            exact_values,
            home: std::env::var_os("HOME").map(PathBuf::from),
            project: project.to_path_buf(),
            temporary_roots: Vec::new(),
            patterns,
        }
    }

    pub fn add_temporary_root(&mut self, path: &Path) {
        self.temporary_roots.push(path.to_path_buf());
    }

    pub fn redact(&self, input: &str) -> String {
        let mut result = input.to_owned();
        for value in &self.exact_values {
            result = result.replace(value, "<REDACTED_ENV_VALUE>");
        }
        for pattern in &self.patterns {
            result = pattern
                .replace_all(&result, "<REDACTED_SECRET_PATTERN>")
                .into_owned();
        }
        if let Some(home) = &self.home {
            result = replace_path(&result, home, "<HOME>");
        }
        result = replace_path(&result, &self.project, "<PROJECT>");
        for temporary in &self.temporary_roots {
            result = replace_path(&result, temporary, "<TEMP>");
        }
        result
    }

    pub fn rule_count(&self) -> usize {
        self.exact_values.len() + self.patterns.len()
    }
}

pub fn is_sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SENSITIVE_NAME_PARTS
        .iter()
        .any(|needle| upper.contains(needle))
}

fn replace_path(input: &str, path: &Path, replacement: &str) -> String {
    let native = path.to_string_lossy();
    let mut result = input.replace(native.as_ref(), replacement);
    let slash = native.replace('\\', "/");
    if slash != native {
        result = result.replace(&slash, replacement);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_secret_values_are_removed() {
        let env = BTreeMap::from([("DEMO_TOKEN".into(), "obviously-invalid-secret".into())]);
        let redactor = Redactor::new(&env, Path::new("/project"));
        let output = redactor.redact("value=obviously-invalid-secret");
        assert!(!output.contains("obviously-invalid-secret"));
        assert!(output.contains("REDACTED_ENV_VALUE"));
    }

    #[test]
    fn common_token_patterns_are_removed() {
        let redactor = Redactor::new(&BTreeMap::new(), Path::new("/project"));
        let fake = "Bearer abcdefghijklmnopqrstuvwxyz";
        assert!(!redactor.redact(fake).contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn ordinary_names_are_not_sensitive() {
        assert!(!is_sensitive_name("PATH"));
        assert!(is_sensitive_name("GITHUB_TOKEN"));
    }
}
