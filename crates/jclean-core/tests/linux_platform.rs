//! The Linux platform layer (spec phase 9), tested on any host against a
//! fixture laid out like a Linux machine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::PathBuf;

use common::{FakeRunner, FakeTrash};
use jclean_core::cancel::CancelToken;
use jclean_core::cleaner::{self, CleanContext, Outcome};
use jclean_core::env::{Env, Os};
use jclean_core::planner::build_plan;
use jclean_core::platform;
use jclean_core::rules::{Audience, Method, RuleSet, paths};
use jclean_core::safety::{NoProcesses, SafetyGuard, protection_for};
use jclean_core::scanner::{ScanMode, ScanOptions, Scanner};
use jclean_core::testing::{age_tree, native, simplify};

struct Machine {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = simplify(fs::canonicalize(tmp.path()).unwrap());
        let home = root.join(native("home/me"));
        fs::create_dir_all(&home).unwrap();
        Self {
            _tmp: tmp,
            root,
            home,
        }
    }

    fn env(&self) -> Env {
        Env::new(&self.home, &self.root, Os::Linux)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(native(rel))
    }

    fn file(&self, rel: &str, bytes: usize) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }
}

fn protected(m: &Machine, rel: &str) -> bool {
    protection_for(&m.path(rel), &platform::protected_paths(&m.env())).is_some()
}

#[test]
fn tokens_follow_xdg() {
    let m = Machine::new();
    let resolve = |env: &Env, p: &str| paths::resolve(p, env).unwrap();
    let env = m.env();
    assert_eq!(
        resolve(&env, "{caches}/pip"),
        Some(m.path("home/me/.cache/pip"))
    );
    assert_eq!(
        resolve(&env, "{config}/Code"),
        Some(m.path("home/me/.config/Code"))
    );
    assert_eq!(
        resolve(&env, "{data}/Trash"),
        Some(m.path("home/me/.local/share/Trash"))
    );
    assert_eq!(
        resolve(&env, "{state}/x"),
        Some(m.path("home/me/.local/state/x"))
    );
    // XDG variables win when they're absolute, and are ignored when relative.
    let moved = env
        .clone()
        .with_var("XDG_CACHE_HOME", m.path("cache-elsewhere"))
        .with_var("XDG_CONFIG_HOME", "relative/config");
    assert_eq!(
        resolve(&moved, "{caches}/pip"),
        Some(m.path("cache-elsewhere/pip"))
    );
    assert_eq!(
        resolve(&moved, "{config}/Code"),
        Some(m.path("home/me/.config/Code"))
    );
    // Windows-only tokens don't apply.
    assert_eq!(resolve(&env, "{localAppData}/x"), None);
}

#[test]
fn system_and_personal_folders_are_protected() {
    let m = Machine::new();
    for rel in [
        "",
        "usr/lib",
        "etc/passwd",
        "var/lib/dpkg",
        "var/cache/apt",
        "var/log",
        "boot",
        "home",
        "home/me",
        "home/me/Documents",
        "home/me/.config",
        "home/me/.local/share",
        "home/me/.local/share/keyrings/login.keyring",
        "home/me/.ssh/id_ed25519",
        "home/me/.mozilla/firefox/profile/logins.json",
        "home/me/.var/app",
    ] {
        assert!(protected(&m, rel), "{rel:?} should be protected");
    }
    for rel in [
        "var/cache/apt/archives",
        "var/cache/apt/archives/x.deb",
        "var/cache/dnf",
        "home/me/.cache/pip",
        "home/me/.local/share/Trash",
        "home/me/.var/app/org.old.App",
    ] {
        assert!(!protected(&m, rel), "{rel:?} should be cleanable");
    }
}

#[test]
fn project_discovery_skips_caches_and_sandboxes() {
    let m = Machine::new();
    let excluded = platform::default_scan_exclusions(&m.env());
    for rel in [
        "home/me/.cache",
        "home/me/.local",
        "home/me/.var",
        "home/me/snap",
    ] {
        assert!(excluded.contains(&m.path(rel)), "{rel}");
    }
}

/// Scans a Linux-shaped machine with the real Linux rules, then dry-runs
/// the clean so every item goes through the SafetyGuard.
#[test]
fn linux_rules_find_and_verify_the_usual_suspects() {
    let m = Machine::new();
    m.file("home/me/.cache/pip/http/a", 80_000);
    m.file("home/me/.npm/_cacache/content-v2/one", 300_000);
    m.file("home/me/.cache/some-app/blob", 50_000);
    m.file("home/me/.cache/thumbnails/large/x.png", 20_000);
    m.file("home/me/.local/share/Trash/files/old.txt", 40_000);
    m.file("home/me/.local/share/Trash/info/old.txt.trashinfo", 100);
    m.file("home/me/.var/app/org.gimp.GIMP/cache/tiles", 60_000);
    m.file("home/me/.var/app/org.gimp.GIMP/config/gimprc", 1_000);
    m.file("home/me/.var/app/org.old.App/data/save", 70_000);
    m.file("var/cache/apt/archives/firefox_1.deb", 900_000);
    m.file("home/me/code/site/package.json", 10);
    m.file("home/me/code/site/node_modules/x/index.js", 40_000);
    age_tree(&m.home, 200).unwrap();

    let env = m.env();
    let rules = RuleSet::builtin(Os::Linux).unwrap();
    let runner = FakeRunner::none()
        .with_tool("flatpak", "org.gimp.GIMP\n")
        .with_tool("apt-get", "")
        .with_tool("npm", "");
    let scanner = Scanner {
        env: &env,
        rules: &rules,
        runner: &runner,
    };
    let opts = ScanOptions::new(ScanMode::Quick, Audience::Developer);
    let scan = scanner.scan(&opts, &CancelToken::new(), &|_| {});

    let found: Vec<&str> = scan.items.iter().map(|i| i.rule_id.as_str()).collect();
    for rule in [
        "linux.python.pip-cache",
        "linux.node.npm-cache",
        "linux.system.app-caches",
        "linux.system.thumbnails",
        "linux.system.trash",
        "linux.system.sandboxed-app-caches",
        "linux.flatpak.leftover-data",
        "linux.system.apt-cache",
        "linux.node.project-dependencies",
    ] {
        assert!(found.contains(&rule), "{rule} missing from {found:?}");
    }
    // Only the uninstalled app's data is a leftover, and it's cleaned by path.
    let leftovers: Vec<_> = scan
        .items
        .iter()
        .filter(|i| i.rule_id == "linux.flatpak.leftover-data")
        .collect();
    assert_eq!(leftovers.len(), 1);
    assert_eq!(
        leftovers[0].path,
        Some(m.path("home/me/.var/app/org.old.App"))
    );
    assert_eq!(leftovers[0].method, Method::Trash);
    // The package cache is cleared by apt itself, as administrator.
    let apt = scan
        .items
        .iter()
        .find(|i| i.rule_id == "linux.system.apt-cache")
        .unwrap();
    assert_eq!(apt.method, Method::Command);
    assert!(apt.cleanable, "{:?}", apt.blocked);

    let ids: Vec<String> = scan
        .items
        .iter()
        .filter(|i| i.cleanable)
        .map(|i| i.id.clone())
        .collect();
    let plan = build_plan(&scan, &rules, &ids, &env, &runner);
    let trash = FakeTrash::new(m.root.join("fake-trash"));
    let guard = SafetyGuard::new(&env);
    let report = cleaner::execute(
        &plan,
        &CleanContext {
            guard: &guard,
            runner: &runner,
            trasher: &trash,
            processes: &NoProcesses,
            history: None,
            scan_id: None,
            dry_run: true,
        },
        &CancelToken::new(),
        &|_| {},
    );
    for o in &report.outcomes {
        assert!(
            matches!(o.outcome, Outcome::WouldClean { .. }),
            "{} was refused: {:?}",
            o.item_id,
            o.outcome
        );
    }
    assert_eq!(report.outcomes.len(), ids.len());
}

/// A disabled snap goes from `snap list --all` to exactly
/// `snap remove <name> --revision=<n>`, and a flag-like name never gets that far.
#[test]
fn old_snap_revisions_become_exact_remove_commands() {
    let m = Machine::new();
    m.file("var/lib/snapd/snaps/core20_1828.snap", 60_000);
    let env = m.env();
    let rules = RuleSet::builtin(Os::Linux).unwrap();
    let runner = FakeRunner::none().with_tool(
        "snap",
        "Name    Version   Rev   Tracking       Publisher   Notes
core20  20230207  1828  latest/stable  canonical✓  base,disabled
core20  20240111  2182  latest/stable  canonical✓  base
",
    );
    let scanner = Scanner {
        env: &env,
        rules: &rules,
        runner: &runner,
    };
    let opts = ScanOptions::new(ScanMode::Quick, Audience::Everyday);
    let scan = scanner.scan(&opts, &CancelToken::new(), &|_| {});
    let item = scan
        .items
        .iter()
        .find(|i| i.rule_id == "linux.snap.disabled-revisions")
        .expect("disabled revision found");
    assert_eq!(item.key, "core20 1828");
    assert!(item.bytes > 0);

    let plan = build_plan(&scan, &rules, std::slice::from_ref(&item.id), &env, &runner);
    let command = plan.items[0].command.as_ref().unwrap();
    assert_eq!(command.args, ["remove", "core20", "--revision=1828"]);
    assert!(plan.needs_admin);

    // A key part that looks like a flag is refused before any command is built.
    let mut evil = scan.clone();
    for i in &mut evil.items {
        if i.rule_id == "linux.snap.disabled-revisions" {
            i.key = "core20 --purge".to_string();
        }
    }
    let plan = build_plan(&evil, &rules, std::slice::from_ref(&item.id), &env, &runner);
    assert!(plan.items.is_empty());
    assert_eq!(plan.skipped.len(), 1);
}
