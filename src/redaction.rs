use crate::model::{Report, RunEvidence};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
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

const BUILTIN_SENSITIVE_LONG_OPTIONS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "api-key",
    "apikey",
    "access-key",
    "accesskey",
    "private-key",
    "authorization",
    "auth",
    "credential",
    "credentials",
    "client-secret",
    "auth-token",
    "access-token",
    "refresh-token",
];

#[derive(Clone)]
struct ExactValue {
    value: String,
    replacement: &'static str,
}

#[derive(Clone)]
pub struct Redactor {
    exact_values: Vec<ExactValue>,
    sensitive_options: BTreeSet<String>,
    home: Option<PathBuf>,
    project: PathBuf,
    temporary_roots: Vec<PathBuf>,
    patterns: Vec<Regex>,
}

impl fmt::Debug for Redactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Redactor")
            .field("exact_rule_count", &self.exact_values.len())
            .field("sensitive_option_count", &self.sensitive_options.len())
            .field("temporary_root_count", &self.temporary_roots.len())
            .field("pattern_rule_count", &self.patterns.len())
            .finish()
    }
}

impl Redactor {
    pub fn new(environment: &BTreeMap<String, String>, project: &Path) -> Self {
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
        let mut redactor = Self {
            exact_values: Vec::new(),
            sensitive_options: BTreeSet::new(),
            home: std::env::var_os("HOME").map(PathBuf::from),
            project: project.to_path_buf(),
            temporary_roots: Vec::new(),
            patterns,
        };
        for (name, value) in environment {
            if is_sensitive_name(name) && value.len() >= 4 {
                redactor.add_exact(value, "<REDACTED_ENV_VALUE>");
            }
        }
        redactor
    }

    pub fn add_commands<'a>(
        &mut self,
        commands: impl IntoIterator<Item = &'a [String]>,
        configured_sensitive_options: &[String],
    ) {
        self.sensitive_options.extend(
            configured_sensitive_options
                .iter()
                .map(|option| option.to_ascii_lowercase()),
        );
        for command in commands {
            self.add_command_values(command);
        }
    }

    pub fn add_temporary_root(&mut self, path: &Path) {
        self.temporary_roots.push(path.to_path_buf());
    }

    pub fn redact(&self, input: &str) -> String {
        let mut result = input.to_owned();
        for exact in &self.exact_values {
            result = result.replace(&exact.value, exact.replacement);
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

    pub fn redact_command(&self, command: &[String]) -> Vec<String> {
        let mut result = Vec::with_capacity(command.len());
        let mut redact_next = false;
        for part in command {
            if redact_next {
                result.push("<REDACTED_CLI_VALUE>".into());
                redact_next = false;
                continue;
            }
            if let Some((option, _value)) = part.split_once('=') {
                if self.is_sensitive_option(option) {
                    result.push(format!("{option}=<REDACTED_CLI_VALUE>"));
                    continue;
                }
            }
            if self.is_sensitive_option(part) {
                result.push(part.clone());
                redact_next = true;
                continue;
            }
            result.push(self.redact_command_part(part));
        }
        result
    }

    pub fn rule_count(&self) -> usize {
        self.exact_values.len() + self.patterns.len()
    }

    pub fn redact_report(&self, report: &mut Report) {
        report.configuration.source = self.redact(&report.configuration.source);
        report.command = self.redact_command(&report.command);
        for run in &mut report.baseline {
            self.redact_run(run);
        }
        for scenario in &mut report.scenarios {
            scenario.name = self.redact(&scenario.name);
            scenario.description = self.redact(&scenario.description);
            for run in &mut scenario.runs {
                self.redact_run(run);
            }
            for restored in &mut scenario.restored_names {
                *restored = self.redact(restored);
            }
            scenario.note = self.redact(&scenario.note);
        }
        for finding in &mut report.findings {
            finding.changed = self.redact(&finding.changed);
            finding.observed = self.redact(&finding.observed);
            finding.conclusion = self.redact(&finding.conclusion);
            finding.next_step = self.redact(&finding.next_step);
            finding.not_proven = self.redact(&finding.not_proven);
            for restored in &mut finding.restored_names {
                *restored = self.redact(restored);
            }
        }
        if let Some(status) = &mut report.workspace_integrity.git_status_before {
            *status = self.redact(status);
        }
        if let Some(status) = &mut report.workspace_integrity.git_status_after {
            *status = self.redact(status);
        }
        report.workspace_integrity.note = self.redact(&report.workspace_integrity.note);
    }

    fn add_command_values(&mut self, command: &[String]) {
        let mut capture_next = false;
        for part in command {
            if capture_next {
                self.add_exact(part, "<REDACTED_CLI_VALUE>");
                capture_next = false;
                continue;
            }
            if let Some((option, value)) = part.split_once('=') {
                if self.is_sensitive_option(option) {
                    self.add_exact(value, "<REDACTED_CLI_VALUE>");
                    continue;
                }
            }
            if self.is_sensitive_option(part) {
                capture_next = true;
            }
        }
    }

    fn add_exact(&mut self, value: &str, replacement: &'static str) {
        if value.is_empty() {
            return;
        }
        if let Some(existing) = self
            .exact_values
            .iter_mut()
            .find(|candidate| candidate.value == value)
        {
            if replacement == "<REDACTED_CLI_VALUE>" {
                existing.replacement = replacement;
            }
            return;
        }
        self.exact_values.push(ExactValue {
            value: value.into(),
            replacement,
        });
        self.exact_values
            .sort_by_key(|exact| std::cmp::Reverse(exact.value.len()));
    }

    fn is_sensitive_option(&self, option: &str) -> bool {
        let lowercase = option.to_ascii_lowercase();
        if self.sensitive_options.contains(&lowercase) {
            return true;
        }
        let Some(long_name) = lowercase.strip_prefix("--") else {
            return false;
        };
        let normalized = long_name.replace(['_', '.'], "-");
        if BUILTIN_SENSITIVE_LONG_OPTIONS.contains(&normalized.as_str()) {
            return true;
        }
        let components: Vec<_> = normalized
            .split('-')
            .filter(|component| !component.is_empty())
            .collect();
        matches!(
            components.last().copied(),
            Some(
                "token"
                    | "secret"
                    | "password"
                    | "passwd"
                    | "authorization"
                    | "auth"
                    | "credential"
                    | "credentials"
            )
        ) || normalized.ends_with("-api-key")
            || normalized.ends_with("-access-key")
            || normalized.ends_with("-private-key")
            || normalized.ends_with("-secret-key")
    }

    fn redact_run(&self, run: &mut RunEvidence) {
        run.stdout_summary = self.redact(&run.stdout_summary);
        run.stderr_summary = self.redact(&run.stderr_summary);
        for check in &mut run.oracle_checks {
            check.check = self.redact(&check.check);
            check.detail = self.redact(&check.detail);
        }
    }

    fn redact_command_part(&self, part: &str) -> String {
        let path = Path::new(part);
        if path.is_absolute() {
            if let Ok(relative) = path.strip_prefix(&self.project) {
                return format!(
                    "<PROJECT>/{}",
                    relative.to_string_lossy().replace('\\', "/")
                );
            }
        }
        let redacted = self.redact(part);
        if redacted != part || !path.is_absolute() {
            return redacted;
        }
        format!(
            "<ABSOLUTE_PATH>/{}",
            path.file_name()
                .map_or_else(|| "item".into(), |name| name.to_string_lossy())
        )
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

    fn command_redactor(command: &[&str], configured: &[&str]) -> Redactor {
        let command: Vec<String> = command.iter().map(|value| (*value).into()).collect();
        let configured: Vec<String> = configured.iter().map(|value| (*value).into()).collect();
        let mut redactor = Redactor::new(&BTreeMap::new(), Path::new("/project"));
        redactor.add_commands([command.as_slice()], &configured);
        redactor
    }

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
    fn command_options_redact_separate_inline_and_case_variant_values() {
        let raw = [
            "tool",
            "--token",
            "fake-token",
            "--password",
            "fake-password",
            "--api-key=fake-api-key",
            "--API_KEY",
            "fake-case-key",
        ];
        let redactor = command_redactor(&raw, &[]);
        let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
        let redacted = redactor.redact_command(&command).join(" ");
        for secret in [
            "fake-token",
            "fake-password",
            "fake-api-key",
            "fake-case-key",
        ] {
            assert!(!redacted.contains(secret));
            assert!(!redactor.redact(&format!("echo {secret}")).contains(secret));
        }
    }

    #[test]
    fn documented_sensitive_option_forms_redact_short_values() {
        for raw in [
            vec!["tool", "--token", "abc"],
            vec!["tool", "--password", "abc"],
            vec!["tool", "--api-key", "abc"],
            vec!["tool", "--api-key=abc"],
            vec!["tool", "--API-KEY=abc"],
        ] {
            let redactor = command_redactor(&raw, &[]);
            let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
            let rendered = redactor.redact_command(&command).join(" ");
            assert!(!rendered.contains("abc"));
            assert!(rendered.contains("REDACTED_CLI_VALUE"));
        }
    }

    #[test]
    fn configured_short_option_is_redacted_without_guessing_other_short_options() {
        let raw = ["tool", "-p", "fake-short", "-j", "4"];
        let redactor = command_redactor(&raw, &["-p"]);
        let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
        assert_eq!(
            redactor.redact_command(&command),
            ["tool", "-p", "<REDACTED_CLI_VALUE>", "-j", "4"]
                .map(String::from)
                .to_vec()
        );
    }

    #[test]
    fn ordinary_and_similarly_named_options_are_not_redacted() {
        let raw = [
            "tool",
            "--output",
            "artifact",
            "--tokenizer",
            "wordpiece",
            "--secretary",
            "name",
        ];
        let redactor = command_redactor(&raw, &[]);
        let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
        assert_eq!(redactor.redact_command(&command), command);
    }

    #[test]
    fn compound_sensitive_long_option_names_are_redacted_by_component() {
        let raw = [
            "tool",
            "--github-token",
            "fake-github",
            "--oauth_token=fake-oauth",
            "--aws-secret-access-key",
            "fake-aws",
        ];
        let redactor = command_redactor(&raw, &[]);
        let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
        let rendered = redactor.redact_command(&command).join(" ");
        for secret in ["fake-github", "fake-oauth", "fake-aws"] {
            assert!(!rendered.contains(secret));
        }
    }

    #[test]
    fn safe_debug_does_not_include_exact_values() {
        let redactor = command_redactor(&["tool", "--token", "fake-debug-secret"], &[]);
        let debug = format!("{redactor:?}");
        assert!(!debug.contains("fake-debug-secret"));
        assert!(debug.contains("exact_rule_count"));
    }

    #[test]
    fn ordinary_names_are_not_sensitive() {
        assert!(!is_sensitive_name("PATH"));
        assert!(is_sensitive_name("GITHUB_TOKEN"));
    }
}
