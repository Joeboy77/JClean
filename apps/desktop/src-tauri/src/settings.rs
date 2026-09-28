//! Settings (spec §5.11, §10): `settings.json` in the app data folder,
//! versioned so later releases can migrate it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jclean_core::env::Env;
use jclean_core::rules::{self, Risk, RuleSet};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};

use crate::engine::{Engine, lock};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum UserMode {
    Everyday,
    Developer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CustomRisk {
    Safe,
    Review,
    Caution,
}

impl From<CustomRisk> for Risk {
    fn from(r: CustomRisk) -> Self {
        match r {
            CustomRisk::Safe => Self::Safe,
            CustomRisk::Review => Self::Review,
            CustomRisk::Caution => Self::Caution,
        }
    }
}

/// A folder added in Settings → Rules. Always moved to the Trash (spec §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomFolder {
    pub id: String,
    pub name: String,
    pub path: String,
    pub risk: CustomRisk,
}

/// An imported rule pack, kept as the JSON it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RulePack {
    pub name: String,
    pub json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    /// First-launch onboarding is done (spec §5.9).
    pub onboarded: bool,
    pub mode: UserMode,
    pub scan_on_launch: bool,
    pub menu_bar_icon: bool,
    /// Show free space as text next to the menu bar icon.
    pub menu_bar_free_space: bool,
    pub launch_at_login: bool,
    /// Where to look for projects and large files. Empty means the home folder.
    pub project_roots: Vec<String>,
    pub excluded_folders: Vec<String>,
    /// 30, 60, 90, 180 or 365 (spec §4.4).
    pub inactive_after_days: u32,
    pub include_external_drives: bool,
    /// Delete user files that need review instead of moving them to the Trash.
    pub delete_user_files: bool,
    /// Ask before cleaning. Always on for caution items.
    pub confirm_before_cleaning: bool,
    pub disabled_rules: Vec<String>,
    pub custom_folders: Vec<CustomFolder>,
    pub rule_packs: Vec<RulePack>,
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            onboarded: false,
            mode: UserMode::Everyday,
            scan_on_launch: true,
            menu_bar_icon: true,
            menu_bar_free_space: false,
            launch_at_login: false,
            project_roots: Vec::new(),
            excluded_folders: Vec::new(),
            inactive_after_days: 90,
            include_external_drives: false,
            delete_user_files: false,
            confirm_before_cleaning: true,
            disabled_rules: Vec::new(),
            custom_folders: Vec::new(),
            rule_packs: Vec::new(),
            check_for_updates: true,
        }
    }
}

const THRESHOLDS: [u32; 5] = [30, 60, 90, 180, 365];

impl Settings {
    /// Reads settings, falling back to defaults. A file that can't be parsed
    /// is kept as `settings.json.bak` rather than silently lost.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<serde_json::Value>(&text).map(migrate) {
            Ok(Ok(settings)) => settings.normalized(),
            _ => {
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let dir = path.parent().ok_or("no settings folder")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    fn normalized(mut self) -> Self {
        if !THRESHOLDS.contains(&self.inactive_after_days) {
            self.inactive_after_days = 90;
        }
        self.version = SETTINGS_VERSION;
        self
    }

    /// Built-in rules plus custom folders and rule packs. Custom entries that
    /// no longer validate (say, a folder that became protected) are dropped.
    pub fn rule_set(&self, builtin: &RuleSet, env: &Env) -> (RuleSet, Vec<String>) {
        let mut set = builtin.clone();
        let mut problems = Vec::new();
        for folder in &self.custom_folders {
            match rules::custom_folder_rule(
                &folder.id,
                &folder.name,
                Path::new(&folder.path),
                folder.risk.into(),
                env,
            )
            .and_then(|r| set.extend(vec![r]))
            {
                Ok(()) => {}
                Err(e) => problems.push(e.to_string()),
            }
        }
        for pack in &self.rule_packs {
            match rules::load_custom(&pack.name, &pack.json, env).and_then(|r| set.extend(r)) {
                Ok(()) => {}
                Err(e) => problems.push(e.to_string()),
            }
        }
        (set, problems)
    }
}

/// Brings a settings file up to date: fields it lacks (older versions)
/// take their defaults, and fields it has that this version doesn't know
/// (a newer JClean) are ignored. Version-specific steps go here as the
/// format changes.
fn migrate(value: serde_json::Value) -> Result<Settings, serde_json::Error> {
    let mut merged = serde_json::to_value(Settings::default())?;
    if let (Some(base), serde_json::Value::Object(saved)) = (merged.as_object_mut(), value) {
        for (key, v) in saved {
            if base.contains_key(&key) {
                base.insert(key, v);
            }
        }
    }
    serde_json::from_value(merged)
}

pub fn settings_path(env: &Env) -> PathBuf {
    jclean_core::platform::app_data_dir(env).join("settings.json")
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(engine: State<'_, Arc<Engine>>) -> Settings {
    lock(&engine.settings).clone()
}

/// Saves settings and applies them: rules, menu bar icon, launch at login.
#[tauri::command]
#[specta::specta]
pub fn save_settings(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    settings: Settings,
) -> Result<Settings, String> {
    let settings = settings.normalized();
    settings.save(&settings_path(&engine.env))?;
    engine.apply_settings(settings.clone());
    crate::tray::apply(&app, &engine);
    crate::system::apply_launch_at_login(&app, settings.launch_at_login);
    Ok(settings)
}

/// Adds a folder chosen in Settings → Rules (spec §6.4).
#[tauri::command]
#[specta::specta]
pub fn add_custom_folder(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    path: String,
    name: String,
    risk: CustomRisk,
) -> Result<Settings, String> {
    let mut settings = lock(&engine.settings).clone();
    let id = unique_id(&name, &settings);
    // Validate before saving: protected folders are refused here, not later.
    rules::custom_folder_rule(&id, &name, Path::new(&path), risk.into(), &engine.env)
        .map_err(|e| e.to_string())?;
    settings.custom_folders.push(CustomFolder {
        id,
        name,
        path,
        risk,
    });
    save_settings(app, engine, settings)
}

/// Adds a rule pack after checking it against the schema and the custom-rule
/// limits (spec §6.4).
#[tauri::command]
#[specta::specta]
pub fn add_rule_pack(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    name: String,
    json: String,
) -> Result<Settings, String> {
    if json.len() > 1_000_000 {
        return Err("That rule pack is too large.".to_string());
    }
    let mut settings = lock(&engine.settings).clone();
    let parsed = rules::load_custom(&name, &json, &engine.env).map_err(|e| e.to_string())?;
    let (current, _) = settings.rule_set(&engine.builtin, &engine.env);
    let mut check = current;
    check.extend(parsed).map_err(|e| e.to_string())?;
    settings.rule_packs.push(RulePack { name, json });
    save_settings(app, engine, settings)
}

/// Removes a custom folder or a rule pack by the rule ID it produced.
#[tauri::command]
#[specta::specta]
pub fn remove_custom_rule(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    rule_id: String,
) -> Result<Settings, String> {
    let mut settings = lock(&engine.settings).clone();
    let os = engine.env.os().as_str();
    settings
        .custom_folders
        .retain(|f| format!("{os}.custom.{}", slug(&f.id)) != rule_id);
    settings.rule_packs.retain(|p| {
        rules::load_custom(&p.name, &p.json, &engine.env)
            .map_or(true, |rules| rules.iter().all(|r| r.id != rule_id))
    });
    save_settings(app, engine, settings)
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn unique_id(name: &str, settings: &Settings) -> String {
    let base = slug(name);
    let base = if base.trim_matches('-').is_empty() {
        "folder".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while settings
        .custom_folders
        .iter()
        .any(|f| slug(&f.id) == slug(&id))
    {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_get_defaults_and_bad_thresholds_are_fixed() {
        let s = migrate(
            serde_json::json!({ "version": 1, "onboarded": true, "inactiveAfterDays": 45 }),
        )
        .unwrap()
        .normalized();
        assert!(s.onboarded);
        assert!(s.menu_bar_icon);
        assert_eq!(s.inactive_after_days, 90);
    }

    #[test]
    fn corrupt_files_are_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert!(dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings {
            onboarded: true,
            mode: UserMode::Developer,
            inactive_after_days: 180,
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn custom_ids_are_unique() {
        let mut s = Settings::default();
        s.custom_folders.push(CustomFolder {
            id: "renders".to_string(),
            name: "Renders".to_string(),
            path: "/x".to_string(),
            risk: CustomRisk::Review,
        });
        assert_eq!(unique_id("Renders", &s), "renders-2");
        assert_eq!(unique_id("???", &Settings::default()), "folder");
    }
}
