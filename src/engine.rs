use crate::config::{Config, OracleConfig};
use crate::fingerprint;
use crate::minimize;
use crate::model::{
    BudgetEvidence, EvidenceLevel, ExecutionRequest, Finding, IntegrityEvidence, Report,
    ReportConfiguration, RunEvidence, ScenarioEvidence, ScenarioStatus,
};
use crate::oracle;
use crate::platform;
use crate::redaction::Redactor;
use crate::report;
use crate::runner;
use crate::scenarios::{self, EnvironmentPlan, ScenarioDefinition, ScenarioKind};
use crate::workspace;
use anyhow::{bail, Context as _, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ResolvedCommand {
    display: Vec<String>,
    executable: PathBuf,
    workspace_relative: Option<PathBuf>,
    args: Vec<String>,
}

#[derive(Debug)]
struct Budget {
    max_runs: usize,
    max_seconds: u64,
    used: usize,
    start: Instant,
    exhausted: bool,
}

impl Budget {
    fn new(config: &Config) -> Self {
        Self {
            max_runs: config.budget.max_total_runs,
            max_seconds: config.budget.max_total_seconds,
            used: 0,
            start: Instant::now(),
            exhausted: false,
        }
    }

    fn take(&mut self) -> bool {
        if self.used >= self.max_runs || self.start.elapsed().as_secs() >= self.max_seconds {
            self.exhausted = true;
            return false;
        }
        self.used += 1;
        true
    }

    fn evidence(&self) -> BudgetEvidence {
        BudgetEvidence {
            max_total_runs: self.max_runs,
            max_total_seconds: self.max_seconds,
            runs_used: self.used,
            elapsed_seconds: self.start.elapsed().as_secs(),
            exhausted: self.exhausted,
        }
    }
}

#[derive(Debug, Clone)]
enum PlanSpec {
    Baseline,
    Scenario(ScenarioDefinition),
    CleanSubset(Vec<String>),
    PathSubset(Vec<PathBuf>),
}

#[derive(Debug)]
enum Attempt {
    Evidence(RunEvidence),
    BudgetExhausted,
}

struct Context<'a> {
    source: &'a Path,
    config: &'a Config,
    command: &'a ResolvedCommand,
    original_environment: &'a BTreeMap<String, String>,
    verbose: bool,
    budget: &'a mut Budget,
}

pub struct EngineOutput {
    pub report: Report,
    pub directory: PathBuf,
}

pub fn check(
    source: &Path,
    config: &Config,
    config_source: &str,
    command_tokens: &[String],
    verbose: bool,
) -> Result<EngineOutput> {
    config.validate()?;
    let source = source
        .canonicalize()
        .context("current project directory could not be resolved")?;
    let original_environment: BTreeMap<String, String> = env::vars().collect();
    let command = resolve_command(&source, command_tokens, &original_environment)?;
    let before_fingerprint = fingerprint::source_fingerprint(&source)?;
    let git_before = fingerprint::git_status(&source);
    let started_at = timestamp();
    let mut budget = Budget::new(config);
    let base_redactor = Redactor::new(&original_environment, &source);

    let mut baseline = Vec::new();
    {
        let mut context = Context {
            source: &source,
            config,
            command: &command,
            original_environment: &original_environment,
            verbose,
            budget: &mut budget,
        };
        for _ in 0..config.run.baseline_runs {
            match run_once(&mut context, PlanSpec::Baseline, "project-copy")? {
                Attempt::Evidence(evidence) => {
                    let interrupted = evidence.interrupted;
                    baseline.push(evidence);
                    if interrupted {
                        break;
                    }
                }
                Attempt::BudgetExhausted => break,
            }
        }
    }

    let baseline_status = baseline_status(&baseline, config);
    let mut scenario_results = Vec::new();
    let mut findings = Vec::new();
    if baseline_status == "STABLE" {
        let selected = scenarios::selected(config);
        let mut context = Context {
            source: &source,
            config,
            command: &command,
            original_environment: &original_environment,
            verbose,
            budget: &mut budget,
        };
        for definition in selected {
            if interrupted(&baseline, &scenario_results) {
                break;
            }
            let (scenario, finding) = run_scenario(&mut context, definition)?;
            if let Some(mut finding) = finding {
                finding.id = format!("AZ-F{:03}", findings.len() + 1);
                findings.push(finding);
            }
            scenario_results.push(scenario);
        }
    }

    let after_fingerprint = fingerprint::source_fingerprint(&source)?;
    let git_after = fingerprint::git_status(&source);
    let source_unchanged = before_fingerprint == after_fingerprint && git_before == git_after;
    let integrity = IntegrityEvidence {
        before_fingerprint: before_fingerprint.clone(),
        after_fingerprint,
        source_unchanged,
        git_status_before: git_before,
        git_status_after: git_after,
        note: "Fingerprint and Git-status comparison exclude .git and AssumeZero's own .assumezero report directory; tested commands ran only in disposable copies.".into(),
    };
    let report = Report {
        schema_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        run_id: ulid::Ulid::new().to_string(),
        started_at,
        finished_at: timestamp(),
        platform: platform::facts(),
        repository_fingerprint: before_fingerprint,
        configuration: ReportConfiguration {
            source: config_source.into(),
            profile: config.scenarios.profile.clone(),
            timeout_seconds: config.run.timeout_seconds,
            baseline_runs: config.run.baseline_runs,
            confirm_failures: config.run.confirm_failures,
            workspace_mode: config.workspace.mode.as_str().into(),
            report_formats: config.report.formats.clone(),
        },
        command: command
            .display
            .iter()
            .map(|part| redact_command_part(part, &base_redactor, &source))
            .collect(),
        baseline,
        baseline_status,
        scenarios: scenario_results,
        findings,
        budget: budget.evidence(),
        redaction_summary: BTreeMap::from([
            ("in_memory_rules".into(), base_redactor.rule_count()),
            ("persisted_environment_values".into(), 0),
        ]),
        workspace_integrity: integrity,
    };
    let directory = report::persist(&source, &report, &config.report.formats)?;
    Ok(EngineOutput { report, directory })
}

fn resolve_command(
    source: &Path,
    tokens: &[String],
    environment: &BTreeMap<String, String>,
) -> Result<ResolvedCommand> {
    let (program, args) = tokens
        .split_first()
        .context("no tested command was provided; use `assumezero check -- <command> [args...]`")?;
    let requested = Path::new(program);
    let (executable, workspace_relative) = if requested.is_absolute()
        || requested.components().count() > 1
    {
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            source.join(requested)
        };
        if !candidate.is_file() {
            bail!("tested command `{program}` does not resolve to a file");
        }
        let relative = candidate.strip_prefix(source).ok().map(Path::to_path_buf);
        (candidate, relative)
    } else {
        let path = platform::environment_value(environment, "PATH").map(std::ffi::OsString::from);
        let resolved = platform::resolve_program(program, path.as_ref())
            .with_context(|| format!("tested command `{program}` was not found on PATH"))?;
        (resolved, None)
    };
    Ok(ResolvedCommand {
        display: tokens.to_vec(),
        executable,
        workspace_relative,
        args: args.to_vec(),
    })
}

fn baseline_status(runs: &[RunEvidence], config: &Config) -> String {
    if runs.len() < config.run.baseline_runs {
        return "BASELINE_FAILED".into();
    }
    let accepted = runs.iter().filter(|run| run.accepted).count();
    if accepted == runs.len() {
        if config.run.strict_output {
            let signatures: BTreeSet<_> = runs
                .iter()
                .map(|run| (&run.stdout_summary, &run.stderr_summary, run.exit_code))
                .collect();
            if signatures.len() != 1 {
                return "BASELINE_UNSTABLE".into();
            }
        }
        "STABLE".into()
    } else if accepted == 0 {
        "BASELINE_FAILED".into()
    } else {
        "BASELINE_UNSTABLE".into()
    }
}

fn run_scenario(
    context: &mut Context<'_>,
    definition: &ScenarioDefinition,
) -> Result<(ScenarioEvidence, Option<Finding>)> {
    if !scenario_supported(definition) {
        return Ok((
            ScenarioEvidence {
                id: definition.id.into(),
                name: definition.name.into(),
                description: definition.description.into(),
                status: ScenarioStatus::SkippedUnsupported,
                best_effort: definition.best_effort,
                runs: Vec::new(),
                restored_names: Vec::new(),
                minimization_complete: false,
                note: "The current platform could not reliably enable this scenario.".into(),
            },
            None,
        ));
    }

    let workspace_name =
        scenarios::workspace_name(definition.kind, context.config.workspace.deep_path_length);
    let mut runs = Vec::new();
    let first = match run_once(context, PlanSpec::Scenario(*definition), &workspace_name) {
        Ok(Attempt::Evidence(evidence)) => evidence,
        Ok(Attempt::BudgetExhausted) => {
            return Ok((
                inconclusive_scenario(
                    definition,
                    "Execution budget was exhausted before the scenario ran.",
                ),
                None,
            ));
        }
        Err(error)
            if matches!(
                definition.kind,
                ScenarioKind::UnicodeWorkdir | ScenarioKind::DeepWorkdir
            ) =>
        {
            return Ok((
                ScenarioEvidence {
                    id: definition.id.into(),
                    name: definition.name.into(),
                    description: definition.description.into(),
                    status: ScenarioStatus::SkippedUnsupported,
                    best_effort: definition.best_effort,
                    runs: Vec::new(),
                    restored_names: Vec::new(),
                    minimization_complete: false,
                    note: format!("The requested path could not be created reliably: {error}"),
                },
                None,
            ));
        }
        Err(error) => {
            return Ok((
                ScenarioEvidence {
                    id: definition.id.into(),
                    name: definition.name.into(),
                    description: definition.description.into(),
                    status: ScenarioStatus::InfrastructureError,
                    best_effort: definition.best_effort,
                    runs: Vec::new(),
                    restored_names: Vec::new(),
                    minimization_complete: false,
                    note: format!("Scenario infrastructure failed: {error}"),
                },
                None,
            ));
        }
    };
    let first_accepted = first.accepted;
    runs.push(first);
    if first_accepted {
        return Ok((
            ScenarioEvidence {
                id: definition.id.into(),
                name: definition.name.into(),
                description: definition.description.into(),
                status: ScenarioStatus::Pass,
                best_effort: definition.best_effort,
                runs,
                restored_names: Vec::new(),
                minimization_complete: false,
                note: if definition.best_effort {
                    "Passed under the process-level best-effort change; this is not an operating-system-wide change.".into()
                } else {
                    "The command remained accepted.".into()
                },
            },
            None,
        ));
    }

    while runs.len() < context.config.run.confirm_failures {
        match run_once(context, PlanSpec::Scenario(*definition), &workspace_name)? {
            Attempt::Evidence(evidence) => runs.push(evidence),
            Attempt::BudgetExhausted => break,
        }
    }
    let stable_failure = runs.len() >= context.config.run.confirm_failures
        && runs.iter().all(|run| !run.accepted && !run.interrupted);
    if !stable_failure {
        let finding = finding_for(
            definition,
            EvidenceLevel::Suspected,
            Vec::new(),
            "The changed result did not repeat enough times within the budget.",
        );
        return Ok((
            ScenarioEvidence {
                id: definition.id.into(),
                name: definition.name.into(),
                description: definition.description.into(),
                status: ScenarioStatus::Inconclusive,
                best_effort: definition.best_effort,
                runs,
                restored_names: Vec::new(),
                minimization_complete: false,
                note: "The failure was not stably confirmed.".into(),
            },
            Some(finding),
        ));
    }

    let (restored_names, minimization_complete, evidence_level, recovery_note) =
        match definition.kind {
            ScenarioKind::CleanEnv => minimize_environment(context)?,
            ScenarioKind::MinimalPath => minimize_path(context)?,
            _ => (
                Vec::new(),
                false,
                EvidenceLevel::Confirmed,
                "The scenario failure repeated, but v0.1.0 does not trace a more specific underlying file or condition for this scenario.".into(),
            ),
        };
    let finding = finding_for(
        definition,
        evidence_level,
        restored_names.clone(),
        &recovery_note,
    );
    Ok((
        ScenarioEvidence {
            id: definition.id.into(),
            name: definition.name.into(),
            description: definition.description.into(),
            status: ScenarioStatus::Fail,
            best_effort: definition.best_effort,
            runs,
            restored_names,
            minimization_complete,
            note: recovery_note,
        },
        Some(finding),
    ))
}

fn minimize_environment(
    context: &mut Context<'_>,
) -> Result<(Vec<String>, bool, EvidenceLevel, String)> {
    let clean = scenarios::clean_environment(context.original_environment, context.config);
    let mut candidates: Vec<String> = context
        .original_environment
        .keys()
        .filter(|name| !platform::contains_environment_name(&clean.values, name))
        .filter(|name| !platform::name_in_list(&context.config.environment.deny, name))
        .cloned()
        .collect();
    candidates.sort();

    if !repeat_accepts(
        context,
        PlanSpec::CleanSubset(candidates.clone()),
        context.config.run.confirm_failures,
    )? {
        return Ok((
            Vec::new(),
            false,
            EvidenceLevel::Confirmed,
            "Restoring the complete candidate environment did not stably recover the command, so no specific variable is claimed.".into(),
        ));
    }

    let minimized = minimize::ddmin(&candidates, |subset| {
        match run_once(
            context,
            PlanSpec::CleanSubset(subset.to_vec()),
            "project-copy",
        )? {
            Attempt::Evidence(evidence) => Ok(Some(evidence.accepted)),
            Attempt::BudgetExhausted => Ok(None),
        }
    })?;
    let final_confirmed = repeat_accepts(
        context,
        PlanSpec::CleanSubset(minimized.items.clone()),
        context.config.run.confirm_failures,
    )?;
    let complete = minimized.complete && final_confirmed;
    let level = if complete {
        EvidenceLevel::Proven
    } else {
        EvidenceLevel::Suspected
    };
    let note = if minimized.items.is_empty() {
        "The recovery experiment did not isolate a named nonessential environment variable.".into()
    } else if complete {
        format!(
            "Restoring only {} made the command pass repeatedly. Values stayed in memory and were not persisted. The set is 1-minimal, not guaranteed globally minimum.",
            minimized.items.join(", ")
        )
    } else {
        format!(
            "The current best recovery set is {}, but the budget or repeated verification prevented a final 1-minimal claim.",
            minimized.items.join(", ")
        )
    };
    Ok((minimized.items, complete, level, note))
}

fn minimize_path(context: &mut Context<'_>) -> Result<(Vec<String>, bool, EvidenceLevel, String)> {
    let original_path = platform::environment_value(context.original_environment, "PATH")
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    let all = platform::deduplicate_paths(platform::split_path(&original_path));
    let top_original = context
        .command
        .executable
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let minimal_original = scenarios::minimal_path(context.config, top_original);
    let candidates: Vec<PathBuf> = all
        .into_iter()
        .filter(|path| !minimal_original.contains(path))
        .collect();

    if !repeat_accepts(
        context,
        PlanSpec::PathSubset(candidates.clone()),
        context.config.run.confirm_failures,
    )? {
        return Ok((
            Vec::new(),
            false,
            EvidenceLevel::Confirmed,
            "Restoring the complete original PATH did not stably recover the command, so no PATH entry is claimed.".into(),
        ));
    }
    let minimized = minimize::ddmin(&candidates, |subset| {
        match run_once(
            context,
            PlanSpec::PathSubset(subset.to_vec()),
            "project-copy",
        )? {
            Attempt::Evidence(evidence) => Ok(Some(evidence.accepted)),
            Attempt::BudgetExhausted => Ok(None),
        }
    })?;
    let final_confirmed = repeat_accepts(
        context,
        PlanSpec::PathSubset(minimized.items.clone()),
        context.config.run.confirm_failures,
    )?;
    let complete = minimized.complete && final_confirmed;
    let home = env::var_os("HOME").map(PathBuf::from);
    let redacted: Vec<String> = minimized
        .items
        .iter()
        .map(|path| platform::redacted_path(path, home.as_deref()))
        .collect();
    let level = if complete {
        EvidenceLevel::Proven
    } else {
        EvidenceLevel::Suspected
    };
    let note = if redacted.is_empty() {
        "The recovery experiment did not isolate a nonessential PATH entry.".into()
    } else if complete {
        format!(
            "Restoring only {} made the command pass repeatedly. The ordered entry set is 1-minimal, not guaranteed globally minimum.",
            redacted.join(", ")
        )
    } else {
        format!(
            "The current best PATH recovery set is {}, but verification was incomplete.",
            redacted.join(", ")
        )
    };
    Ok((redacted, complete, level, note))
}

fn repeat_accepts(context: &mut Context<'_>, plan: PlanSpec, times: usize) -> Result<bool> {
    let mut accepted = 0;
    for _ in 0..times {
        match run_once(context, plan.clone(), "project-copy")? {
            Attempt::Evidence(evidence) if evidence.accepted => accepted += 1,
            Attempt::Evidence(_) | Attempt::BudgetExhausted => return Ok(false),
        }
    }
    Ok(accepted == times)
}

fn run_once(
    context: &mut Context<'_>,
    plan_spec: PlanSpec,
    workspace_name: &str,
) -> Result<Attempt> {
    let isolated = workspace::create(context.source, &context.config.workspace, workspace_name)?;
    let project = isolated.project();
    let executable = context.command.workspace_relative.as_ref().map_or_else(
        || context.command.executable.clone(),
        |relative| project.join(relative),
    );
    let top_directory = executable.parent().unwrap_or(project);
    let plan = match plan_spec {
        PlanSpec::Baseline => {
            scenarios::baseline_environment(context.original_environment, context.config)
        }
        PlanSpec::Scenario(definition) => {
            prepare_scenario_directories(definition.kind, isolated.temporary_root())?;
            scenarios::apply(
                &definition,
                context.original_environment,
                context.config,
                isolated.temporary_root(),
                top_directory,
            )
            .context("scenario is unsupported")?
        }
        PlanSpec::CleanSubset(names) => {
            let mut clean =
                scenarios::clean_environment(context.original_environment, context.config);
            for name in names {
                if let Some(value) = context.original_environment.get(&name) {
                    platform::set_environment_value(&mut clean.values, &name, value.clone());
                }
            }
            clean
        }
        PlanSpec::PathSubset(paths) => {
            let mut baseline =
                scenarios::baseline_environment(context.original_environment, context.config);
            let joined = platform::deduplicate_paths(
                scenarios::minimal_path(context.config, top_directory)
                    .into_iter()
                    .chain(paths),
            );
            let value = platform::join_path(&joined)
                .context("PATH entries could not be represented on this platform")?;
            platform::set_environment_value(
                &mut baseline.values,
                "PATH",
                value.to_string_lossy().into_owned(),
            );
            baseline
        }
    };

    for prepare in &context.config.run.prepare {
        if prepare.is_empty() {
            continue;
        }
        if !context.budget.take() {
            return Ok(Attempt::BudgetExhausted);
        }
        let raw = run_tokens(
            context.source,
            project,
            prepare,
            &plan,
            context.config.run.timeout_seconds,
            context.config.report.log_limit_bytes,
            context.verbose,
        )?;
        let redactor = redactor_for(context.original_environment, context.source, &isolated);
        let evidence = oracle::evaluate(raw, &OracleConfig::default(), project, |text| {
            redactor.redact(text)
        })?;
        if !evidence.accepted {
            bail!(
                "prepare command `{}` failed in an isolated workspace",
                prepare.join(" ")
            );
        }
    }
    if !context.budget.take() {
        return Ok(Attempt::BudgetExhausted);
    }
    let raw = runner::execute(&ExecutionRequest {
        executable,
        args: context.command.args.clone(),
        cwd: project.to_path_buf(),
        env: plan.values,
        clear_env: plan.clear,
        timeout_seconds: context.config.run.timeout_seconds,
        log_limit_bytes: context.config.report.log_limit_bytes,
        verbose: context.verbose,
    })?;
    let redactor = redactor_for(context.original_environment, context.source, &isolated);
    Ok(Attempt::Evidence(oracle::evaluate(
        raw,
        &context.config.oracle,
        project,
        |text| redactor.redact(text),
    )?))
}

fn run_tokens(
    source: &Path,
    workspace: &Path,
    tokens: &[String],
    plan: &EnvironmentPlan,
    timeout_seconds: u64,
    log_limit_bytes: usize,
    verbose: bool,
) -> Result<crate::model::RawExecution> {
    let command = resolve_command(source, tokens, &plan.values)?;
    let executable = command
        .workspace_relative
        .map_or(command.executable, |relative| workspace.join(relative));
    runner::execute(&ExecutionRequest {
        executable,
        args: command.args,
        cwd: workspace.to_path_buf(),
        env: plan.values.clone(),
        clear_env: plan.clear,
        timeout_seconds,
        log_limit_bytes,
        verbose,
    })
}

fn redactor_for(
    environment: &BTreeMap<String, String>,
    source: &Path,
    isolated: &workspace::IsolatedWorkspace,
) -> Redactor {
    let mut redactor = Redactor::new(environment, source);
    redactor.add_temporary_root(isolated.temporary_root());
    redactor
}

fn prepare_scenario_directories(kind: ScenarioKind, root: &Path) -> Result<()> {
    match kind {
        ScenarioKind::EmptyHome => fs::create_dir_all(root.join("empty-home"))?,
        ScenarioKind::EmptyCache => {
            for name in ["xdg", "npm", "pip", "uv", "gradle"] {
                fs::create_dir_all(root.join("empty-cache").join(name))?;
            }
        }
        ScenarioKind::RedirectedTemp => fs::create_dir_all(root.join("redirected-temp"))?,
        _ => {}
    }
    Ok(())
}

fn scenario_supported(definition: &ScenarioDefinition) -> bool {
    match definition.kind {
        ScenarioKind::TimezoneUtc => !cfg!(windows),
        ScenarioKind::LocaleC => scenarios::locale_c_supported(),
        _ => true,
    }
}

fn inconclusive_scenario(definition: &ScenarioDefinition, note: &str) -> ScenarioEvidence {
    ScenarioEvidence {
        id: definition.id.into(),
        name: definition.name.into(),
        description: definition.description.into(),
        status: ScenarioStatus::Inconclusive,
        best_effort: definition.best_effort,
        runs: Vec::new(),
        restored_names: Vec::new(),
        minimization_complete: false,
        note: note.into(),
    }
}

fn finding_for(
    definition: &ScenarioDefinition,
    evidence: EvidenceLevel,
    restored_names: Vec<String>,
    recovery_note: &str,
) -> Finding {
    let conclusion = match definition.kind {
        ScenarioKind::CleanEnv if !restored_names.is_empty() => format!(
            "The project has an undeclared dependency on the presence of {}.",
            restored_names.join(", ")
        ),
        ScenarioKind::MinimalPath if !restored_names.is_empty() => format!(
            "A child process has an undeclared dependency on tooling reachable through {}.",
            restored_names.join(", ")
        ),
        _ => format!(
            "The command has a repeatable dependency exposed by {} within the supported scenario.",
            definition.name
        ),
    };
    Finding {
        id: String::new(),
        scenario_id: definition.id.into(),
        evidence,
        changed: definition.description.into(),
        observed: recovery_note.into(),
        conclusion,
        next_step: next_step(definition.kind).into(),
        not_proven: "This result does not prove a unique global root cause, behavior outside the supported scenario, or safety of the tested command.".into(),
        restored_names,
    }
}

fn next_step(kind: ScenarioKind) -> &'static str {
    match kind {
        ScenarioKind::CleanEnv => {
            "Declare the required variable by name in setup documentation or make the command discover its dependency without ambient state."
        }
        ScenarioKind::MinimalPath => {
            "Declare and install the child tool explicitly, or invoke it through the project's dependency manager."
        }
        ScenarioKind::EmptyHome => {
            "Move required configuration into the repository or document the required user-level setup."
        }
        ScenarioKind::EmptyCache => {
            "Ensure preparation can populate dependencies from a clean cache and document any offline requirement."
        }
        ScenarioKind::SpaceWorkdir | ScenarioKind::UnicodeWorkdir | ScenarioKind::DeepWorkdir => {
            "Quote and normalize paths, avoid fixed absolute paths, and add this path shape to project tests."
        }
        ScenarioKind::RedirectedTemp => {
            "Use platform temporary-directory APIs and avoid relying on residual temporary files."
        }
        ScenarioKind::TimezoneUtc => {
            "Make timezone behavior explicit and test with fixed timezone-aware inputs."
        }
        ScenarioKind::LocaleC => {
            "Make locale-sensitive parsing or formatting explicit and test supported locales."
        }
    }
}

fn interrupted(baseline: &[RunEvidence], scenarios: &[ScenarioEvidence]) -> bool {
    baseline.iter().any(|run| run.interrupted)
        || scenarios
            .iter()
            .flat_map(|scenario| &scenario.runs)
            .any(|run| run.interrupted)
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_else(|_| "0".into(), |value| value.as_secs().to_string())
}

fn redact_command_part(part: &str, redactor: &Redactor, project: &Path) -> String {
    let path = Path::new(part);
    if path.is_absolute() {
        if let Ok(relative) = path.strip_prefix(project) {
            return format!(
                "<PROJECT>/{}",
                relative.to_string_lossy().replace('\\', "/")
            );
        }
    }
    let redacted = redactor.redact(part);
    if redacted != part || !path.is_absolute() {
        return redacted;
    }
    format!(
        "<ABSOLUTE_PATH>/{}",
        path.file_name()
            .map_or_else(|| "item".into(), |name| name.to_string_lossy())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(accepted: bool, output: &str) -> RunEvidence {
        RunEvidence {
            accepted,
            exit_code: Some(i32::from(!accepted)),
            duration_ms: 1,
            timed_out: false,
            interrupted: false,
            output_truncated: false,
            stdout_summary: output.into(),
            stderr_summary: String::new(),
            oracle_checks: vec![],
        }
    }

    #[test]
    fn stable_baseline_requires_all_runs_to_pass() {
        let config = Config::default();
        assert_eq!(
            baseline_status(&[evidence(true, ""), evidence(true, "")], &config),
            "STABLE"
        );
        assert_eq!(
            baseline_status(&[evidence(true, ""), evidence(false, "")], &config),
            "BASELINE_UNSTABLE"
        );
        assert_eq!(
            baseline_status(&[evidence(false, ""), evidence(false, "")], &config),
            "BASELINE_FAILED"
        );
    }

    #[test]
    fn strict_output_detects_instability() {
        let mut config = Config::default();
        config.run.strict_output = true;
        assert_eq!(
            baseline_status(&[evidence(true, "a"), evidence(true, "b")], &config),
            "BASELINE_UNSTABLE"
        );
    }
}
