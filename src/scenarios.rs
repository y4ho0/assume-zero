use crate::config::Config;
use crate::platform;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioKind {
    EmptyHome,
    EmptyCache,
    CleanEnv,
    MinimalPath,
    SpaceWorkdir,
    UnicodeWorkdir,
    DeepWorkdir,
    RedirectedTemp,
    TimezoneUtc,
    LocaleC,
}

#[derive(Debug, Clone, Copy)]
pub struct ScenarioDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub kind: ScenarioKind,
    pub quick: bool,
    pub best_effort: bool,
}

pub const ALL: &[ScenarioDefinition] = &[
    ScenarioDefinition {
        id: "AZ-S001",
        name: "EMPTY_HOME",
        description: "Redirect user-home and user-data variables to an empty directory.",
        kind: ScenarioKind::EmptyHome,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S002",
        name: "EMPTY_CACHE",
        description: "Redirect safely controlled dependency cache variables.",
        kind: ScenarioKind::EmptyCache,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S003",
        name: "CLEAN_ENV",
        description: "Retain only platform essentials and the configured allowlist.",
        kind: ScenarioKind::CleanEnv,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S004",
        name: "MINIMAL_PATH",
        description: "Retain the top-level command directory and essential system paths.",
        kind: ScenarioKind::MinimalPath,
        quick: false,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S005",
        name: "SPACE_WORKDIR",
        description: "Run from a copied workspace whose path contains multiple spaces.",
        kind: ScenarioKind::SpaceWorkdir,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S006",
        name: "UNICODE_WORKDIR",
        description: "Run from a copied workspace whose path contains Unicode.",
        kind: ScenarioKind::UnicodeWorkdir,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S007",
        name: "DEEP_WORKDIR",
        description: "Run from a safely bounded deep copied workspace path.",
        kind: ScenarioKind::DeepWorkdir,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S008",
        name: "REDIRECTED_TEMP",
        description: "Redirect process temporary-directory variables.",
        kind: ScenarioKind::RedirectedTemp,
        quick: true,
        best_effort: false,
    },
    ScenarioDefinition {
        id: "AZ-S009",
        name: "TIMEZONE_UTC",
        description: "Set TZ=UTC where process-level timezone selection is supported.",
        kind: ScenarioKind::TimezoneUtc,
        quick: false,
        best_effort: true,
    },
    ScenarioDefinition {
        id: "AZ-S010",
        name: "LOCALE_C",
        description: "Set LANG=C and LC_ALL=C where that locale is available.",
        kind: ScenarioKind::LocaleC,
        quick: false,
        best_effort: false,
    },
];

#[derive(Debug, Clone)]
pub struct EnvironmentPlan {
    pub values: BTreeMap<String, String>,
    pub clear: bool,
}

pub fn selected(config: &Config) -> Vec<&'static ScenarioDefinition> {
    let included: BTreeSet<String> = config
        .scenarios
        .include
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect();
    let excluded: BTreeSet<String> = config
        .scenarios
        .exclude
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect();
    ALL.iter()
        .filter(|scenario| {
            let selected_by_profile = config.scenarios.profile == "deep" || scenario.quick;
            let explicitly_included =
                included.contains(scenario.id) || included.contains(scenario.name);
            let explicitly_excluded =
                excluded.contains(scenario.id) || excluded.contains(scenario.name);
            (selected_by_profile || explicitly_included) && !explicitly_excluded
        })
        .collect()
}

pub fn baseline_environment(
    original: &BTreeMap<String, String>,
    config: &Config,
) -> EnvironmentPlan {
    let denied: BTreeSet<_> = config.environment.deny.iter().collect();
    EnvironmentPlan {
        values: original
            .iter()
            .filter(|(name, _)| !denied.contains(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        clear: true,
    }
}

pub fn clean_environment(original: &BTreeMap<String, String>, config: &Config) -> EnvironmentPlan {
    let mut keep = platform::necessary_environment();
    keep.extend(config.environment.preserve.iter().cloned());
    let denied: BTreeSet<_> = config.environment.deny.iter().collect();
    EnvironmentPlan {
        values: original
            .iter()
            .filter(|(name, _)| keep.contains(*name) && !denied.contains(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        clear: true,
    }
}

pub fn workspace_name(kind: ScenarioKind, deep_target: usize) -> String {
    match kind {
        ScenarioKind::SpaceWorkdir => "AssumeZero Test Workspace/project copy".into(),
        ScenarioKind::UnicodeWorkdir => "项目-测试-Δ".into(),
        ScenarioKind::DeepWorkdir => {
            let mut result = String::from("deep");
            let mut index = 0;
            while result.len() < deep_target {
                result.push_str(&format!("/segment-{index:03}"));
                index += 1;
            }
            result
        }
        _ => "project-copy".into(),
    }
}

pub fn apply(
    definition: &ScenarioDefinition,
    original: &BTreeMap<String, String>,
    config: &Config,
    scenario_root: &Path,
    top_program_directory: &Path,
) -> Option<EnvironmentPlan> {
    let mut plan = baseline_environment(original, config);
    match definition.kind {
        ScenarioKind::EmptyHome => {
            let empty = scenario_root.join("empty-home");
            let value = empty.to_string_lossy().into_owned();
            for name in applicable_home_variables() {
                plan.values.insert((*name).into(), value.clone());
            }
        }
        ScenarioKind::EmptyCache => {
            for (name, directory) in [
                ("XDG_CACHE_HOME", "xdg"),
                ("npm_config_cache", "npm"),
                ("PIP_CACHE_DIR", "pip"),
                ("UV_CACHE_DIR", "uv"),
                ("GRADLE_USER_HOME", "gradle"),
            ] {
                plan.values.insert(
                    name.into(),
                    scenario_root
                        .join("empty-cache")
                        .join(directory)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        ScenarioKind::CleanEnv => return Some(clean_environment(original, config)),
        ScenarioKind::MinimalPath => {
            let paths = minimal_path(config, top_program_directory);
            plan.values.insert(
                "PATH".into(),
                platform::join_path(&paths)?.to_string_lossy().into_owned(),
            );
        }
        ScenarioKind::RedirectedTemp => {
            let value = scenario_root
                .join("redirected-temp")
                .to_string_lossy()
                .into_owned();
            for name in ["TMP", "TEMP", "TMPDIR"] {
                plan.values.insert(name.into(), value.clone());
            }
        }
        ScenarioKind::TimezoneUtc => {
            #[cfg(windows)]
            return None;
            #[cfg(not(windows))]
            plan.values.insert("TZ".into(), "UTC".into());
        }
        ScenarioKind::LocaleC => {
            if !locale_c_supported() {
                return None;
            }
            plan.values.insert("LC_ALL".into(), "C".into());
            plan.values.insert("LANG".into(), "C".into());
        }
        ScenarioKind::SpaceWorkdir | ScenarioKind::UnicodeWorkdir | ScenarioKind::DeepWorkdir => {}
    }
    Some(plan)
}

pub fn minimal_path(config: &Config, top_program_directory: &Path) -> Vec<PathBuf> {
    platform::deduplicate_paths(
        std::iter::once(top_program_directory.to_path_buf())
            .chain(platform::minimal_system_path())
            .chain(config.environment.preserve_path_entries.clone()),
    )
}

fn applicable_home_variables() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
        ]
    }
    #[cfg(not(windows))]
    {
        &["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"]
    }
}

pub fn locale_c_supported() -> bool {
    #[cfg(windows)]
    {
        false
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("locale")
            .arg("-a")
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .any(|line| line == "C" || line == "POSIX" || line.starts_with("C."))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_profile_selects_seven_scenarios() {
        assert_eq!(selected(&Config::default()).len(), 7);
    }

    #[test]
    fn deep_profile_selects_all_scenarios() {
        let mut config = Config::default();
        config.scenarios.profile = "deep".into();
        assert_eq!(selected(&config).len(), 10);
    }

    #[test]
    fn clean_environment_retains_allowlist_and_not_other_values() {
        let original = BTreeMap::from([
            ("REQUIRED_DEMO".into(), "yes".into()),
            ("CI".into(), "true".into()),
        ]);
        let mut config = Config::default();
        config.environment.preserve.push("CI".into());
        let clean = clean_environment(&original, &config);
        assert_eq!(clean.values.get("CI").map(String::as_str), Some("true"));
        assert!(!clean.values.contains_key("REQUIRED_DEMO"));
    }
}
