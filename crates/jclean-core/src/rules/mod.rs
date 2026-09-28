//! Detection rules: data, not code (spec §6). Built-in rule files under
//! `rules/<os>/` are embedded at build time; user rules load at runtime and
//! are held to stricter limits (spec §6.4).

pub mod keep;
pub mod model;
pub mod paths;

use std::collections::HashSet;
use std::sync::OnceLock;

pub use model::*;

use crate::env::{Env, Os};

include!(concat!(env!("OUT_DIR"), "/builtin_rules.rs"));

const SCHEMA_JSON: &str = include_str!("../../../../rules/schema/rule.schema.json");

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("{file}: invalid JSON: {source}")]
    Json {
        file: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{file}: doesn't match the rule schema: {}", errors.join("; "))]
    Schema { file: String, errors: Vec<String> },
    #[error("rule {id}: {reason}")]
    Invalid { id: String, reason: String },
    #[error("rule ID {0} is used more than once")]
    DuplicateId(String),
}

fn invalid(rule: &Rule, reason: impl Into<String>) -> RuleError {
    RuleError::Invalid {
        id: rule.id.clone(),
        reason: reason.into(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    /// All built-in rules for one OS.
    pub fn builtin(os: Os) -> Result<Self, RuleError> {
        let mut set = Self::default();
        for (file_os, name, json) in BUILTIN_RULE_FILES {
            if *file_os == os.as_str() {
                let rules = parse_file(&format!("{file_os}/{name}"), json, RuleSource::Builtin)?;
                set.extend(rules)?;
            }
        }
        Ok(set)
    }

    pub fn from_rules(rules: Vec<Rule>) -> Result<Self, RuleError> {
        let mut set = Self::default();
        set.extend(rules)?;
        Ok(set)
    }

    /// Adds rules, rejecting duplicate IDs.
    pub fn extend(&mut self, rules: Vec<Rule>) -> Result<(), RuleError> {
        let mut ids: HashSet<String> = self.rules.iter().map(|r| r.id.clone()).collect();
        for rule in &rules {
            if !ids.insert(rule.id.clone()) {
                return Err(RuleError::DuplicateId(rule.id.clone()));
            }
        }
        self.rules.extend(rules);
        Ok(())
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn get(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Rules shown in a mode: Everyday shows `everyday` rules; Developer shows all.
    pub fn active(&self, os: Os, audience: Audience) -> impl Iterator<Item = &Rule> {
        self.rules.iter().filter(move |r| {
            r.platforms.contains(&os)
                && (audience == Audience::Developer || r.audience.contains(&Audience::Everyday))
        })
    }
}

/// Parses a rule file (a JSON array), checks it against the schema, then
/// checks each rule's meaning.
pub fn parse_file(file: &str, json: &str, source: RuleSource) -> Result<Vec<Rule>, RuleError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| RuleError::Json {
        file: file.to_string(),
        source: e,
    })?;
    check_schema(file, &value)?;
    let mut rules: Vec<Rule> = serde_json::from_value(value).map_err(|e| RuleError::Json {
        file: file.to_string(),
        source: e,
    })?;
    for rule in &mut rules {
        rule.source = source;
        validate(rule)?;
    }
    Ok(rules)
}

/// A user rule pack or custom folder (spec §6.4): trash only, fixed paths
/// only, nothing protected.
pub fn load_custom(file: &str, json: &str, env: &Env) -> Result<Vec<Rule>, RuleError> {
    let rules = parse_file(file, json, RuleSource::Custom)?;
    let protected = crate::platform::protected_paths(env);
    for rule in &rules {
        let Detect::Fixed { paths, .. } = &rule.detect else {
            return Err(invalid(rule, "custom rules can only list folders"));
        };
        for pattern in paths {
            let resolved =
                paths::resolve(pattern, env).map_err(|e| invalid(rule, e.to_string()))?;
            if let Some(path) = resolved {
                let literal = paths::literal_prefix(&path);
                if crate::safety::protection_for(&literal, &protected).is_some() {
                    return Err(invalid(
                        rule,
                        format!("{} is protected and can't be cleaned", literal.display()),
                    ));
                }
            }
        }
    }
    Ok(rules)
}

/// A folder the user added in Settings → Rules (spec §6.4): always moved to
/// the Trash, never a protected path, marked "Custom".
pub fn custom_folder_rule(
    id: &str,
    name: &str,
    path: &std::path::Path,
    risk: Risk,
    env: &Env,
) -> Result<Rule, RuleError> {
    let slug: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let rule = Rule {
        id: format!("{}.custom.{slug}", env.os().as_str()),
        version: 1,
        platforms: vec![env.os()],
        audience: vec![Audience::Everyday, Audience::Developer],
        category: Category::Other,
        group: "Custom folders".to_string(),
        labels: Labels {
            developer: name.to_string(),
            everyday: name.to_string(),
        },
        description: Description {
            what: format!("A folder you added: {}", path.display()),
            if_cleared: "It moves to the Trash, so you can put it back until you empty the Trash."
                .to_string(),
        },
        icon: "folder".to_string(),
        risk,
        regenerates: false,
        detect: Detect::Fixed {
            paths: vec![path.display().to_string()],
            each_child: false,
            exclude: Vec::new(),
        },
        unused: Unused::default(),
        cleanup: Cleanup {
            method: Method::Trash,
            command: None,
            fallback: None,
            requires_admin: false,
            keep_root: false,
        },
        related_apps: Vec::new(),
        may_share_blocks: false,
        cross_filesystems: false,
        keep: None,
        docs: None,
        source: RuleSource::Custom,
    };
    if risk == Risk::Info || !path.is_absolute() {
        return Err(invalid(
            &rule,
            "pick a folder, and a risk of safe, review or caution",
        ));
    }
    validate(&rule)?;
    let protected = crate::platform::protected_paths(env);
    if crate::safety::protection_for(path, &protected).is_some() {
        return Err(invalid(
            &rule,
            format!("{} is protected and can't be cleaned", path.display()),
        ));
    }
    Ok(rule)
}

pub fn schema() -> &'static serde_json::Value {
    static SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(SCHEMA_JSON).unwrap_or(serde_json::Value::Null))
}

fn check_schema(file: &str, value: &serde_json::Value) -> Result<(), RuleError> {
    static VALIDATOR: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    let validator = VALIDATOR
        .get_or_init(|| jsonschema::validator_for(schema()).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| RuleError::Schema {
            file: "rule.schema.json".to_string(),
            errors: vec![e.clone()],
        })?;
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(RuleError::Schema {
            file: file.to_string(),
            errors,
        })
    }
}

/// Checks that go beyond the schema: combinations that would be unsafe or
/// meaningless.
pub fn validate(rule: &Rule) -> Result<(), RuleError> {
    let mut parts = rule.id.split('.');
    let os = parts.next().unwrap_or_default();
    let segments_ok = rule.id.split('.').count() == 3
        && rule.id.split('.').all(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    if !segments_ok || !rule.platforms.iter().any(|p| p.as_str() == os) {
        return Err(invalid(
            rule,
            "ID must be <os>.<ecosystem>.<name> and match a platform",
        ));
    }

    let c = &rule.cleanup;
    if (rule.risk == Risk::Info) != (c.method == Method::None) {
        return Err(invalid(
            rule,
            "info rules, and only info rules, use method none",
        ));
    }
    if (c.method == Method::Command) != c.command.is_some() {
        return Err(invalid(
            rule,
            "method command needs a command, and only it may have one",
        ));
    }
    if c.fallback.is_some() && c.method != Method::Command {
        return Err(invalid(rule, "only command cleanups have a fallback"));
    }
    if matches!(c.fallback, Some(Method::Command | Method::None)) {
        return Err(invalid(rule, "fallback must be delete or trash"));
    }
    if rule.risk == Risk::Caution
        && (c.method == Method::Delete || c.fallback == Some(Method::Delete))
    {
        return Err(invalid(rule, "caution items can't be deleted permanently"));
    }
    if c.keep_root
        && matches!(
            &rule.detect,
            Detect::Fixed {
                each_child: true,
                ..
            }
        )
    {
        return Err(invalid(rule, "keepRoot has no effect with eachChild"));
    }

    let env = Env::new("/h", "/", os_of(rule));
    let check_glob = |g: &str| {
        paths::matcher(g)
            .map(|_| ())
            .map_err(|e| invalid(rule, e.to_string()))
    };
    match &rule.detect {
        Detect::Fixed { paths, exclude, .. } => {
            for p in paths {
                paths::resolve(p, &env).map_err(|e| invalid(rule, e.to_string()))?;
            }
            exclude.iter().try_for_each(|g| check_glob(g))?;
        }
        Detect::Query { paths, names, .. } => {
            for p in paths {
                paths::resolve(p, &env).map_err(|e| invalid(rule, e.to_string()))?;
            }
            names.iter().try_for_each(|g| check_glob(g))?;
        }
        Detect::ProjectArtifact { markers, folders } => {
            markers
                .iter()
                .chain(folders)
                .try_for_each(|g| check_glob(g))?;
        }
        Detect::Probe { probe } => {
            if !crate::probes::KNOWN_PROBES.contains(&probe.as_str()) {
                return Err(invalid(rule, format!("unknown probe {probe}")));
            }
        }
    }

    if rule.source == RuleSource::Custom {
        if c.method != Method::Trash {
            return Err(invalid(rule, "custom rules always move items to the Trash"));
        }
        if !matches!(rule.detect, Detect::Fixed { .. }) {
            return Err(invalid(rule, "custom rules can only list folders"));
        }
    }
    Ok(())
}

fn os_of(rule: &Rule) -> Os {
    rule.platforms.first().copied().unwrap_or(Os::Macos)
}
