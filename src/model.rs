use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScenarioStatus {
    Pass,
    Fail,
    SkippedUnsupported,
    Inconclusive,
    InfrastructureError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceLevel {
    Proven,
    Confirmed,
    Suspected,
    Inconclusive,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleCheck {
    pub check: String,
    pub accepted: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvidence {
    pub accepted: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub timed_out: bool,
    pub interrupted: bool,
    pub output_truncated: bool,
    pub stdout_summary: String,
    pub stderr_summary: String,
    pub oracle_checks: Vec<OracleCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioEvidence {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ScenarioStatus,
    pub best_effort: bool,
    pub runs: Vec<RunEvidence>,
    pub restored_names: Vec<String>,
    pub minimization_complete: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub scenario_id: String,
    pub evidence: EvidenceLevel,
    pub changed: String,
    pub observed: String,
    pub conclusion: String,
    pub next_step: String,
    pub not_proven: String,
    pub restored_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEvidence {
    pub max_total_runs: usize,
    pub max_total_seconds: u64,
    pub runs_used: usize,
    pub elapsed_seconds: u64,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityEvidence {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub source_unchanged: bool,
    pub git_status_before: Option<String>,
    pub git_status_after: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfiguration {
    pub source: String,
    pub profile: String,
    pub timeout_seconds: u64,
    pub baseline_runs: usize,
    pub confirm_failures: usize,
    pub workspace_mode: String,
    pub report_formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool_version: String,
    pub run_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub platform: BTreeMap<String, String>,
    pub repository_fingerprint: String,
    pub configuration: ReportConfiguration,
    pub command: Vec<String>,
    pub baseline: Vec<RunEvidence>,
    pub baseline_status: String,
    pub scenarios: Vec<ScenarioEvidence>,
    pub findings: Vec<Finding>,
    pub budget: BudgetEvidence,
    pub redaction_summary: BTreeMap<String, usize>,
    pub workspace_integrity: IntegrityEvidence,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub clear_env: bool,
    pub timeout_seconds: u64,
    pub log_limit_bytes: usize,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct RawExecution {
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub timed_out: bool,
    pub interrupted: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub output_truncated: bool,
}
