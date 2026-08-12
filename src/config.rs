use crate::platform;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "version_one")]
    pub version: u32,
    #[serde(default)]
    pub run: RunConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub oracle: OracleConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub scenarios: ScenariosConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
    #[serde(default)]
    pub report: ReportConfig,
}

const fn version_one() -> u32 {
    1
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            run: RunConfig::default(),
            workspace: WorkspaceConfig::default(),
            oracle: OracleConfig::default(),
            environment: EnvironmentConfig::default(),
            scenarios: ScenariosConfig::default(),
            budget: BudgetConfig::default(),
            report: ReportConfig::default(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub prepare: Vec<Vec<String>>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_baseline_runs")]
    pub baseline_runs: usize,
    #[serde(default = "default_confirm_failures")]
    pub confirm_failures: usize,
    #[serde(default)]
    pub strict_output: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            prepare: Vec::new(),
            timeout_seconds: default_timeout(),
            baseline_runs: default_baseline_runs(),
            confirm_failures: default_confirm_failures(),
            strict_output: false,
        }
    }
}

const fn default_timeout() -> u64 {
    300
}
const fn default_baseline_runs() -> usize {
    2
}
const fn default_confirm_failures() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceMode {
    WorkingTree,
    GitClean,
}

impl WorkspaceMode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WorkingTree => "working-tree",
            Self::GitClean => "git-clean",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default = "default_workspace_mode")]
    pub mode: WorkspaceMode,
    #[serde(default = "default_max_size")]
    pub max_size_mib: u64,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub include_untracked: Vec<PathBuf>,
    #[serde(default)]
    pub allow_external_symlinks: bool,
    #[serde(default = "default_deep_path")]
    pub deep_path_length: usize,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            mode: default_workspace_mode(),
            max_size_mib: default_max_size(),
            max_entries: default_max_entries(),
            exclude: default_excludes(),
            include_untracked: Vec::new(),
            allow_external_symlinks: false,
            deep_path_length: default_deep_path(),
        }
    }
}

fn default_workspace_mode() -> WorkspaceMode {
    WorkspaceMode::WorkingTree
}
const fn default_max_size() -> u64 {
    2_048
}
const fn default_max_entries() -> usize {
    100_000
}
fn default_excludes() -> Vec<String> {
    vec![".git".into(), ".assumezero".into()]
}
const fn default_deep_path() -> usize {
    180
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleConfig {
    #[serde(default = "default_oracle_kind")]
    pub kind: String,
    #[serde(default = "default_exit_codes")]
    pub accepted_exit_codes: Vec<i32>,
    #[serde(default)]
    pub stdout_contains: Vec<String>,
    #[serde(default)]
    pub stderr_not_contains: Vec<String>,
    #[serde(default)]
    pub stdout_regex: Option<String>,
    #[serde(default)]
    pub required_files: Vec<PathBuf>,
    #[serde(default)]
    pub forbidden_files: Vec<PathBuf>,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            kind: default_oracle_kind(),
            accepted_exit_codes: default_exit_codes(),
            stdout_contains: Vec::new(),
            stderr_not_contains: Vec::new(),
            stdout_regex: None,
            required_files: Vec::new(),
            forbidden_files: Vec::new(),
        }
    }
}
fn default_oracle_kind() -> String {
    "exit-code".into()
}
fn default_exit_codes() -> Vec<i32> {
    vec![0]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub preserve: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub preserve_path_entries: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenariosConfig {
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub pairwise: bool,
}

impl Default for ScenariosConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            include: Vec::new(),
            exclude: Vec::new(),
            pairwise: false,
        }
    }
}
fn default_profile() -> String {
    "quick".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    #[serde(default = "default_max_runs")]
    pub max_total_runs: usize,
    #[serde(default = "default_max_seconds")]
    pub max_total_seconds: u64,
}
impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_total_runs: default_max_runs(),
            max_total_seconds: default_max_seconds(),
        }
    }
}
const fn default_max_runs() -> usize {
    40
}
const fn default_max_seconds() -> u64 {
    1_800
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportConfig {
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    #[serde(default = "default_true")]
    pub redact_home: bool,
    #[serde(default = "default_log_limit")]
    pub log_limit_bytes: usize,
    #[serde(default)]
    pub sensitive_options: Vec<String>,
}
impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            formats: default_formats(),
            redact_home: true,
            log_limit_bytes: default_log_limit(),
            sensitive_options: Vec::new(),
        }
    }
}
fn default_formats() -> Vec<String> {
    vec!["terminal".into(), "json".into()]
}
const fn default_true() -> bool {
    true
}
const fn default_log_limit() -> usize {
    131_072
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| {
            format!("configuration file `{}` could not be read", path.display())
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| {
            let location = error.span().map_or_else(
                || "at an unknown location".into(),
                |span| format!("near byte offset {}", span.start),
            );
            anyhow::anyhow!(
                "configuration error in `{}` {location}; parser source excerpts are suppressed because configuration values may be sensitive\n\nFix example:\n  \
                 version = 1\n  [run]\n  command = [\"cargo\", \"test\"]",
                path.display()
            )
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported configuration field `version`; actual value suppressed because configuration values may be sensitive. Use `version = 1`");
        }
        if self.run.timeout_seconds == 0 {
            bail!("field `run.timeout_seconds` must be at least 1");
        }
        if self.run.baseline_runs < 2 {
            bail!("field `run.baseline_runs` must be at least 2");
        }
        if self.run.confirm_failures == 0 {
            bail!("field `run.confirm_failures` must be at least 1");
        }
        if self.run.prepare.iter().any(Vec::is_empty) {
            bail!("field `run.prepare` may not contain an empty command array");
        }
        if self.budget.max_total_runs < self.run.baseline_runs {
            bail!("field `budget.max_total_runs` must cover all baseline runs");
        }
        if self.budget.max_total_seconds == 0 {
            bail!("field `budget.max_total_seconds` must be at least 1");
        }
        if self.workspace.max_size_mib == 0 {
            bail!("field `workspace.max_size_mib` must be at least 1");
        }
        if self.workspace.max_entries == 0 {
            bail!("field `workspace.max_entries` must be at least 1");
        }
        if self.workspace.deep_path_length < 32 {
            bail!("field `workspace.deep_path_length` must be at least 32");
        }
        if self.workspace.deep_path_length > 240 {
            bail!("field `workspace.deep_path_length` must not exceed 240");
        }
        for path in &self.workspace.include_untracked {
            validate_relative_path("workspace.include_untracked", path)?;
        }
        if self.oracle.kind != "exit-code" {
            bail!("field `oracle.kind` supports only `exit-code` in v0.1.0");
        }
        if self.oracle.accepted_exit_codes.is_empty() {
            bail!("field `oracle.accepted_exit_codes` must not be empty");
        }
        if let Some(pattern) = &self.oracle.stdout_regex {
            regex::Regex::new(pattern).map_err(|_| {
                anyhow::anyhow!(
                    "field `oracle.stdout_regex` contains an invalid regex; pattern suppressed because configuration values may be sensitive"
                )
            })?;
        }
        for path in self
            .oracle
            .required_files
            .iter()
            .chain(&self.oracle.forbidden_files)
        {
            validate_relative_path("oracle file list", path)?;
        }
        if !matches!(self.scenarios.profile.as_str(), "quick" | "deep") {
            bail!("field `scenarios.profile` must be `quick` or `deep`");
        }
        for (index, scenario) in self
            .scenarios
            .include
            .iter()
            .chain(&self.scenarios.exclude)
            .enumerate()
        {
            if !valid_scenario_name(scenario) {
                bail!(
                    "field `scenarios.include/exclude[{index}]` contains an unknown scenario; value suppressed because configuration values may be sensitive. Run `assumezero list-scenarios` for stable IDs and names"
                );
            }
        }
        if self.scenarios.pairwise {
            bail!(
                "field `scenarios.pairwise = true` is reserved but not enabled in v0.1.0; \
                 use stable single scenarios"
            );
        }
        let formats: BTreeSet<_> = ["terminal", "json", "markdown", "junit"]
            .into_iter()
            .collect();
        for (index, format) in self.report.formats.iter().enumerate() {
            if !formats.contains(format.as_str()) {
                bail!(
                    "field `report.formats[{index}]` contains an unsupported format; value suppressed because configuration values may be sensitive. Choose terminal, json, markdown, or junit"
                );
            }
        }
        if !self.report.redact_home {
            bail!(
                "field `report.redact_home` must be true in configuration v1; home-path redaction is a mandatory privacy boundary"
            );
        }
        if self.report.log_limit_bytes < 1_024 {
            bail!("field `report.log_limit_bytes` must be at least 1024");
        }
        for (index, option) in self.report.sensitive_options.iter().enumerate() {
            let option_name = option
                .strip_prefix("--")
                .or_else(|| option.strip_prefix('-'))
                .unwrap_or_default();
            if option_name.is_empty()
                || option_name.starts_with('-')
                || option_name.contains('=')
                || option_name.chars().any(char::is_whitespace)
            {
                bail!(
                    "field `report.sensitive_options[{index}]` contains an invalid option token; value suppressed because it may contain sensitive data. Use a token such as `-p` or `--custom-key`"
                );
            }
        }
        Ok(())
    }
}

fn validate_relative_path(field: &str, path: &Path) -> Result<()> {
    if !platform::is_portable_relative_path(path) {
        bail!(
            "field `{field}` contains an unsafe path; value suppressed because configuration values may be sensitive. Use a non-empty relative path without `..`"
        );
    }
    Ok(())
}

fn valid_scenario_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "AZ-S001"
            | "EMPTY_HOME"
            | "AZ-S002"
            | "EMPTY_CACHE"
            | "AZ-S003"
            | "CLEAN_ENV"
            | "AZ-S004"
            | "MINIMAL_PATH"
            | "AZ-S005"
            | "SPACE_WORKDIR"
            | "AZ-S006"
            | "UNICODE_WORKDIR"
            | "AZ-S007"
            | "DEEP_WORKDIR"
            | "AZ-S008"
            | "REDIRECTED_TEMP"
            | "AZ-S009"
            | "TIMEZONE_UTC"
            | "AZ-S010"
            | "LOCALE_C"
    )
}

pub const EXAMPLE_CONFIG: &str = r#"version = 1

[run]
command = ["cargo", "test"]
prepare = []
timeout_seconds = 300
baseline_runs = 2
confirm_failures = 2

[workspace]
mode = "working-tree"
max_size_mib = 2048
max_entries = 100000
exclude = [".git", ".assumezero"]
include_untracked = []

[oracle]
kind = "exit-code"
accepted_exit_codes = [0]

[environment]
preserve = ["CI"]
deny = []

[scenarios]
profile = "quick"
include = []
exclude = []

[budget]
max_total_runs = 40
max_total_seconds = 1800

[report]
formats = ["terminal", "json", "markdown"]
redact_home = true
sensitive_options = []
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse() {
        let config: Config = toml::from_str("version = 1").expect("valid config");
        assert_eq!(config.run.baseline_runs, 2);
        assert_eq!(config.scenarios.profile, "quick");
        assert_eq!(config.workspace.max_entries, 100_000);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = toml::from_str::<Config>("version = 1\ntyop = true")
            .err()
            .expect("unknown field must fail");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn invalid_regex_is_rejected_before_execution() {
        let mut config = Config::default();
        config.oracle.stdout_regex = Some("(".into());
        assert!(config.validate().is_err());
    }

    #[test]
    fn home_path_redaction_cannot_be_disabled() {
        let mut config = Config::default();
        config.report.redact_home = false;
        assert!(config.validate().is_err());
    }

    #[test]
    fn unsafe_untracked_path_is_rejected() {
        let mut config = Config::default();
        config
            .workspace
            .include_untracked
            .push(PathBuf::from("../outside"));
        assert!(config.validate().is_err());

        config.workspace.include_untracked = vec![PathBuf::from("C:outside")];
        assert!(config.validate().is_err());
    }

    #[test]
    fn sensitive_short_options_require_explicit_option_tokens() {
        let mut config = Config::default();
        config.report.sensitive_options.push("-p".into());
        assert!(config.validate().is_ok());
        config.report.sensitive_options = vec!["password".into()];
        assert!(config.validate().is_err());
    }

    #[test]
    fn unknown_scenario_name_is_rejected() {
        let mut config = Config::default();
        config.scenarios.include.push("AZ-S999".into());
        assert!(config.validate().is_err());
    }
}
