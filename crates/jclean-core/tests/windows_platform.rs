//! The Windows platform layer (spec §7.2, §8.3), tested on any host against
//! a fixture laid out like a Windows drive.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use jclean_core::env::{Env, Os};
use jclean_core::platform::{self, Scope};
use jclean_core::rules::{Category, paths};
use jclean_core::safety::{Refusal, Root, SafetyGuard, protection_for};
use jclean_core::tools::find_tool;

/// `root` is the drive (C:\), `home` is C:\Users\me.
struct Drive {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Drive {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let home = root.join("Users/me");
        for dir in [
            "Windows/System32",
            "Windows/SoftwareDistribution/Download/abc123",
            "Program Files/nodejs",
            "ProgramData",
            "Users/me/AppData/Local/npm-cache/_cacache",
            "Users/me/AppData/Roaming/npm",
            "Users/me/AppData/Roaming/Microsoft/Protect/S-1-5-21",
            "Users/me/Documents",
            "Users/me/OneDrive - Contoso/Reports",
            "Users/me/projects/app/node_modules",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(
            root.join("Windows/SoftwareDistribution/Download/abc123/update.cab"),
            b"x",
        )
        .unwrap();
        Self {
            _tmp: tmp,
            root,
            home,
        }
    }

    fn env(&self) -> Env {
        Env::new(&self.home, &self.root, Os::Windows)
            .with_var("TEMP", self.home.join("AppData/Local/Temp"))
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn protected(d: &Drive, rel: &str) -> bool {
    protection_for(&d.path(rel), &platform::protected_paths(&d.env())).is_some()
}

#[test]
fn tokens_resolve_to_windows_folders() {
    let d = Drive::new();
    let env = d.env();
    let resolve = |p: &str| paths::resolve(p, &env).unwrap();
    assert_eq!(
        resolve("{caches}/npm-cache"),
        Some(d.home.join("AppData/Local/npm-cache"))
    );
    assert_eq!(
        resolve("{localAppData}/Yarn/Cache"),
        Some(d.home.join("AppData/Local/Yarn/Cache"))
    );
    assert_eq!(
        resolve("{appData}/npm"),
        Some(d.home.join("AppData/Roaming/npm"))
    );
    assert_eq!(resolve("{temp}"), Some(d.home.join("AppData/Local/Temp")));
    // No Windows equivalent: the rule path simply doesn't apply.
    assert_eq!(resolve("{logs}/DiagnosticReports"), None);
}

#[test]
fn env_var_names_are_case_insensitive_on_windows() {
    let env = Env::new("/h", "/r", Os::Windows).with_var("SYSTEMROOT", "/r/Windows");
    assert_eq!(env.var("SystemRoot"), Some("/r/Windows".as_ref()));
    let mac = Env::new("/h", "/r", Os::Macos).with_var("SYSTEMROOT", "/x");
    assert_eq!(mac.var("SystemRoot"), None);
}

#[test]
fn system_and_personal_folders_are_protected() {
    let d = Drive::new();
    for rel in [
        "",
        "Windows",
        "Windows/System32",
        "Windows/SoftwareDistribution",
        "Windows/SoftwareDistribution/Download",
        "Program Files/nodejs",
        "Program Files (x86)",
        "ProgramData",
        "Users",
        "Users/me",
        "Users/me/Documents",
        "Users/me/AppData",
        "Users/me/AppData/Local",
        "Users/me/AppData/Roaming/Microsoft/Protect/S-1-5-21",
        "Users/me/OneDrive - Contoso",
        "Users/me/.ssh/id_ed25519",
    ] {
        assert!(protected(&d, rel), "{rel:?} should be protected");
    }
}

#[test]
fn only_the_insides_of_carved_out_folders_can_be_cleaned() {
    let d = Drive::new();
    // Windows Update's downloads are a cleanup target inside C:\Windows.
    assert!(!protected(
        &d,
        "Windows/SoftwareDistribution/Download/abc123"
    ));
    // Being inside the carve-out's parent isn't enough.
    assert!(protected(&d, "Windows/SoftwareDistribution/DataStore"));
    let carve = platform::protected_paths(&d.env())
        .into_iter()
        .filter(|p| p.scope == Scope::CleanInside)
        .count();
    assert_eq!(carve, 1);
}

#[test]
fn ordinary_caches_are_not_protected() {
    let d = Drive::new();
    for rel in [
        "Users/me/AppData/Local/npm-cache/_cacache",
        "Users/me/projects/app/node_modules",
        "Windows.old",
    ] {
        assert!(!protected(&d, rel), "{rel:?} should be cleanable");
    }
}

#[test]
fn the_guard_accepts_windows_update_downloads_and_refuses_the_rest() {
    let d = Drive::new();
    let guard = SafetyGuard::new(&d.env());
    let download = d.path("Windows/SoftwareDistribution/Download");
    let roots = [Root {
        path: download.clone(),
        inclusive: false,
    }];
    assert!(guard.check_path(&download.join("abc123"), &roots).is_ok());
    assert_eq!(
        guard.check_path(&download, &roots),
        Err(Refusal::OutsideRuleRoot)
    );
    // A rule can't claim the Windows folder by naming it as its root.
    let windows = [Root {
        path: d.path("Windows"),
        inclusive: true,
    }];
    assert_eq!(
        guard.check_path(&d.path("Windows/System32"), &windows),
        Err(Refusal::Protected)
    );
}

#[test]
fn cloud_folders_are_left_alone() {
    let d = Drive::new();
    let env = d.env();
    let cloud = platform::cloud_dirs(&env);
    assert!(
        cloud.contains(&d.home.join("OneDrive - Contoso")),
        "{cloud:?}"
    );
    let excluded = platform::default_scan_exclusions(&env);
    assert!(excluded.contains(&d.home.join("AppData")));
    assert!(excluded.contains(&d.home.join("OneDrive - Contoso")));
}

#[test]
fn tools_are_found_with_windows_extensions() {
    let d = Drive::new();
    let env = d.env();
    fs::write(d.path("Program Files/nodejs/npm.cmd"), b"").unwrap();
    fs::write(d.path("Windows/System32/Dism.exe"), b"").unwrap();
    assert_eq!(
        find_tool(&env, "npm"),
        Some(d.path("Program Files/nodejs/npm.cmd"))
    );
    assert_eq!(
        find_tool(&env, "Dism"),
        Some(d.path("Windows/System32/Dism.exe"))
    );
    assert_eq!(find_tool(&env, "docker"), None);
}

#[test]
fn the_disk_map_colours_windows_folders() {
    let d = Drive::new();
    let env = d.env();
    let cat = |p: &Path| platform::categorize(&env, p, Category::Other);
    assert_eq!(cat(&d.home.join("Videos")), Category::Media);
    assert_eq!(cat(&d.home.join("Downloads")), Category::Documents);
    assert_eq!(cat(&d.home.join("OneDrive - Contoso")), Category::Documents);
    assert_eq!(cat(&d.home.join("AppData/Local")), Category::Apps);
    assert_eq!(
        cat(&d.home.join("projects/app/node_modules")),
        Category::Developer
    );
    assert_eq!(cat(&d.path("Program Files/nodejs")), Category::Apps);
    assert_eq!(cat(&d.path("Windows")), Category::System);
}

#[test]
fn app_data_lives_in_local_app_data() {
    let d = Drive::new();
    assert_eq!(
        platform::app_data_dir(&d.env()),
        d.home.join("AppData/Local/app.jclean")
    );
}
