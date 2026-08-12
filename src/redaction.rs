use crate::model::{Report, RunEvidence};
use aho_corasick::{AhoCorasick, MatchKind};
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

const MAX_LITERAL_RULES: usize = 2_048;
const MAX_LITERAL_RULE_BYTES: usize = 256 * 1_024;
const REDACTION_BUDGET_MARKER: &str = "<REDACTION_RULE_BUDGET_EXCEEDED>";

#[derive(Clone)]
pub struct Redactor {
    literal_rules: BTreeMap<String, &'static str>,
    literal_rule_bytes: usize,
    literal_matcher: Option<AhoCorasick>,
    literal_replacements: Vec<&'static str>,
    literal_rule_budget_exhausted: bool,
    sensitive_options: BTreeSet<String>,
    project: PathBuf,
    temporary_roots: Vec<PathBuf>,
    patterns: Vec<Regex>,
}

impl fmt::Debug for Redactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Redactor")
            .field("literal_rule_count", &self.literal_rules.len())
            .field(
                "literal_rule_budget_exhausted",
                &self.literal_rule_budget_exhausted,
            )
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
            literal_rules: BTreeMap::new(),
            literal_rule_bytes: 0,
            literal_matcher: None,
            literal_replacements: Vec::new(),
            literal_rule_budget_exhausted: false,
            sensitive_options: BTreeSet::new(),
            project: project.to_path_buf(),
            temporary_roots: Vec::new(),
            patterns,
        };
        redactor.add_path_value(project, "<PROJECT>");
        for (name, value) in environment {
            if matches!(name.to_ascii_uppercase().as_str(), "HOME" | "USERPROFILE")
                && !value.is_empty()
            {
                redactor.add_path_value(Path::new(value), "<HOME>");
            }
        }
        for name in ["HOME", "USERPROFILE"] {
            if let Some(value) = std::env::var_os(name) {
                redactor.add_path_value(Path::new(&value), "<HOME>");
            }
        }
        for (name, value) in environment {
            if is_sensitive_name(name) && !value.is_empty() {
                redactor.add_exact(value, "<REDACTED_ENV_VALUE>");
            }
        }
        redactor.rebuild_literal_matcher();
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
        self.rebuild_literal_matcher();
    }

    pub fn add_temporary_root(&mut self, path: &Path) {
        self.temporary_roots.push(path.to_path_buf());
        self.add_path_value(path, "<TEMP>");
        self.rebuild_literal_matcher();
    }

    pub fn redact(&self, input: &str) -> String {
        if self.literal_rule_budget_exhausted {
            return REDACTION_BUDGET_MARKER.into();
        }
        let mut result = self.redact_exact_values(input);
        for pattern in &self.patterns {
            result = pattern
                .replace_all(&result, "<REDACTED_SECRET_PATTERN>")
                .into_owned();
        }
        result
    }

    fn redact_exact_values(&self, input: &str) -> String {
        let Some(matcher) = &self.literal_matcher else {
            return input.into();
        };
        let mut result = String::with_capacity(input.len());
        let mut offset = 0;
        for matched in matcher.find_iter(input) {
            result.push_str(&input[offset..matched.start()]);
            let replacement = self.literal_replacements[matched.pattern().as_usize()];
            let matched_length = matched.end() - matched.start();
            if replacement.len() <= matched_length {
                result.push_str(replacement);
            } else {
                result.extend(std::iter::repeat('*').take(matched_length));
            }
            offset = matched.end();
        }
        result.push_str(&input[offset..]);
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
            if let Some((option, _value)) = self.attached_sensitive_short(part) {
                result.push(format!("{option}<REDACTED_CLI_VALUE>"));
                continue;
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
        self.literal_rules.len() + self.patterns.len()
    }

    pub const fn rule_budget_exhausted(&self) -> bool {
        self.literal_rule_budget_exhausted
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

    pub fn redact_loaded_report(&self, report: &mut Report) {
        self.redact_report(report);
        report.tool_version = self.redact(&report.tool_version);
        let platform = std::mem::take(&mut report.platform);
        for (key, value) in platform {
            let key = if matches!(key.as_str(), "os" | "arch" | "family") {
                key
            } else {
                self.redact(&key)
            };
            report.platform.insert(key, self.redact(&value));
        }
        for format in &mut report.configuration.report_formats {
            *format = self.redact(format);
        }
        let summary = std::mem::take(&mut report.redaction_summary);
        for (key, value) in summary {
            let key = self.redact(&key);
            let entry = report.redaction_summary.entry(key).or_default();
            *entry = entry.saturating_add(value);
        }
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
            if let Some((_option, value)) = self.attached_sensitive_short(part) {
                self.add_exact(value, "<REDACTED_CLI_VALUE>");
                continue;
            }
            if self.is_sensitive_option(part) {
                capture_next = true;
            }
        }
    }

    fn attached_sensitive_short<'a>(&self, part: &'a str) -> Option<(&'a str, &'a str)> {
        let lowercase = part.to_ascii_lowercase();
        self.sensitive_options.iter().find_map(|configured| {
            let name = configured.strip_prefix('-')?;
            if configured.starts_with("--")
                || name.chars().count() != 1
                || lowercase.len() <= configured.len()
                || !lowercase.starts_with(configured)
            {
                return None;
            }
            Some(part.split_at(configured.len()))
        })
    }

    fn add_exact(&mut self, value: &str, replacement: &'static str) {
        if value.is_empty() {
            return;
        }
        if let Some(existing) = self.literal_rules.get_mut(value) {
            if replacement == "<REDACTED_CLI_VALUE>" {
                *existing = replacement;
            }
            return;
        }
        self.add_literal_rule(value, replacement);
    }

    fn add_path_value(&mut self, path: &Path, replacement: &'static str) {
        let native = path.to_string_lossy();
        if !native.is_empty() {
            self.add_literal_rule(native.as_ref(), replacement);
        }
        let slash = native.replace('\\', "/");
        if !slash.is_empty() {
            self.add_literal_rule(&slash, replacement);
        }
    }

    fn add_literal_rule(&mut self, value: &str, replacement: &'static str) {
        if self.literal_rule_budget_exhausted || self.literal_rules.contains_key(value) {
            return;
        }
        let Some(next_bytes) = self.literal_rule_bytes.checked_add(value.len()) else {
            self.literal_rule_budget_exhausted = true;
            return;
        };
        if self.literal_rules.len() >= MAX_LITERAL_RULES || next_bytes > MAX_LITERAL_RULE_BYTES {
            self.literal_rule_budget_exhausted = true;
            return;
        }
        self.literal_rules.insert(value.into(), replacement);
        self.literal_rule_bytes = next_bytes;
    }

    fn rebuild_literal_matcher(&mut self) {
        if self.literal_rule_budget_exhausted || self.literal_rules.is_empty() {
            self.literal_matcher = None;
            self.literal_replacements.clear();
            return;
        }
        let patterns: Vec<&str> = self.literal_rules.keys().map(String::as_str).collect();
        self.literal_replacements = self.literal_rules.values().copied().collect();
        self.literal_matcher = match AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(patterns)
        {
            Ok(matcher) => Some(matcher),
            Err(_) => {
                self.literal_rule_budget_exhausted = true;
                self.literal_replacements.clear();
                None
            }
        };
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
                let relative = relative.to_string_lossy().replace('\\', "/");
                return format!("<PROJECT>/{}", self.redact(&relative));
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
    fn short_sensitive_environment_values_are_removed() {
        let env = BTreeMap::from([("DEMO_TOKEN".into(), "abc".into())]);
        let redactor = Redactor::new(&env, Path::new("/project"));
        let output = redactor.redact("fixture-secret=abc");
        assert_eq!(output, "fixture-secret=***");
    }

    #[test]
    fn exact_replacements_do_not_rescan_inserted_markers_or_expand_output() {
        let env = BTreeMap::from([
            ("A_TOKEN".into(), "A".into()),
            ("D_TOKEN".into(), "D".into()),
            ("E_TOKEN".into(), "E".into()),
            ("N_TOKEN".into(), "N".into()),
            ("R_TOKEN".into(), "R".into()),
            ("V_TOKEN".into(), "V".into()),
        ]);
        let redactor = Redactor::new(&env, Path::new("/project"));
        assert_eq!(redactor.redact("A"), "*");
    }

    #[test]
    fn repeated_short_values_are_redacted_without_output_growth() {
        let env = BTreeMap::from([
            ("X_TOKEN".into(), "x".into()),
            ("Z_TOKEN".into(), "z".into()),
        ]);
        let redactor = Redactor::new(&env, Path::new("/project"));
        let input = "x".repeat(50_000);
        let output = redactor.redact(&input);
        assert_eq!(output.len(), input.len());
        assert!(output.bytes().all(|byte| byte == b'*'));
    }

    #[test]
    fn overlapping_and_multibyte_literals_use_leftmost_longest_matches() {
        let env = BTreeMap::from([
            ("SHORT_TOKEN".into(), "abc".into()),
            ("LONG_TOKEN".into(), "abcd".into()),
            ("UNICODE_TOKEN".into(), "秘密".into()),
        ]);
        let redactor = Redactor::new(&env, Path::new("/project"));
        assert_eq!(redactor.redact("abcd"), "****");
        assert_eq!(redactor.redact("秘密"), "******");
    }

    #[test]
    fn excessive_literal_rules_fail_closed() {
        let env: BTreeMap<String, String> = (0..=MAX_LITERAL_RULES)
            .map(|index| (format!("TOKEN_{index}"), format!("fake-secret-{index}")))
            .collect();
        let redactor = Redactor::new(&env, Path::new("/project"));
        assert!(redactor.rule_budget_exhausted());
        assert_eq!(
            redactor.redact("ordinary evidence"),
            REDACTION_BUDGET_MARKER
        );
    }

    #[test]
    fn windows_userprofile_paths_are_redacted_portably() {
        let environment =
            BTreeMap::from([("UserProfile".into(), r"C:\Users\AssumeZeroExample".into())]);
        let redactor = Redactor::new(&environment, Path::new("/project"));
        let rendered = redactor.redact(r"path=C:\Users\AssumeZeroExample\settings.toml");
        assert_eq!(rendered, r"path=<HOME>\settings.toml");
    }

    #[test]
    fn project_relative_path_segments_are_exact_value_redacted() {
        let project = tempfile::tempdir().expect("project");
        let secret_path = project.path().join("abc").to_string_lossy().into_owned();
        let command = vec!["tool".into(), "--token".into(), "abc".into(), secret_path];
        let mut redactor = Redactor::new(&BTreeMap::new(), project.path());
        redactor.add_commands([command.as_slice()], &[]);
        let rendered = redactor.redact_command(&command).join(" ");
        assert!(!rendered.contains("abc"));
        assert!(rendered.contains("<PROJECT>/***"));
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
    fn configured_short_option_redacts_attached_values() {
        let raw = ["tool", "-pfake-attached", "-j4"];
        let redactor = command_redactor(&raw, &["-p"]);
        let command: Vec<String> = raw.iter().map(|part| (*part).into()).collect();
        assert_eq!(
            redactor.redact_command(&command),
            ["tool", "-p<REDACTED_CLI_VALUE>", "-j4"]
                .map(String::from)
                .to_vec()
        );
        assert!(!redactor
            .redact("echo fake-attached")
            .contains("fake-attached"));
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
        assert!(debug.contains("literal_rule_count"));
    }

    #[test]
    fn ordinary_names_are_not_sensitive() {
        assert!(!is_sensitive_name("PATH"));
        assert!(is_sensitive_name("GITHUB_TOKEN"));
    }
}
