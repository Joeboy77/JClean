//! The safety suite (spec §16). It must stay exhaustive: every way a path
//! could escape its rule's roots, or be something the user needs, is here.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::FakeProcesses;
use jclean_core::cancel::CancelToken;
use jclean_core::safety::{NoProcesses, Refusal, Root, SafetyGuard, Snapshot, Target};
use jclean_core::sizing::{InodeSet, MeasureOptions, measure};
use jclean_core::testing::Fixture;
use proptest::prelude::*;

struct Setup {
    _tmp: tempfile::TempDir,
    f: Fixture,
    guard: SafetyGuard,
    cache: PathBuf,
}

/// A fixture with a cache folder (the rule root) and precious things outside it.
fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::new(tmp.path()).unwrap();
    f.file("Documents/thesis.docx", 50_000).unwrap();
    f.file(".ssh/id_ed25519", 400).unwrap();
    f.file("Library/Caches/app/blob", 100_000).unwrap();
    let cache = f.path("Library/Caches");
    let guard = SafetyGuard::new(&f.env());
    Setup {
        _tmp: tmp,
        f,
        guard,
        cache,
    }
}

fn inside(root: &Path) -> Vec<Root> {
    vec![Root {
        path: root.to_path_buf(),
        inclusive: false,
    }]
}

fn snapshot(path: &Path) -> Snapshot {
    let m = measure(
        path,
        &MeasureOptions::default(),
        &InodeSet::new(),
        &CancelToken::new(),
    )
    .unwrap();
    Snapshot {
        allocated: m.allocated,
        own_mtime: m.own_mtime,
    }
}

fn check(s: &Setup, path: &Path, roots: &[Root]) -> Result<jclean_core::safety::Verified, Refusal> {
    let snap = fs::symlink_metadata(path).ok().map_or(
        Snapshot {
            allocated: 0,
            own_mtime: 0,
        },
        |_| snapshot(path),
    );
    s.guard.check(
        &Target {
            path,
            roots,
            snapshot: snap,
            regenerates: true,
            cross_filesystems: false,
            exclude: &[],
            related_apps: &[],
        },
        &NoProcesses,
        &CancelToken::new(),
    )
}

#[test]
fn approves_an_ordinary_cache_folder() {
    let s = setup();
    let v = check(&s, &s.cache.join("app"), &inside(&s.cache)).unwrap();
    assert!(v.is_dir && !v.is_symlink);
    assert!(v.allocated >= 100_000);
}

#[test]
fn a_symlink_is_approved_as_a_link_never_followed() {
    let s = setup();
    let link =
        s.f.symlink("Library/Caches/escape", &s.f.path("Documents"))
            .unwrap();
    let v = check(&s, &link, &inside(&s.cache)).unwrap();
    assert!(v.is_symlink);
    assert_eq!(v.path, link);
    assert_eq!(v.allocated, 0);
}

#[test]
fn a_symlink_in_a_parent_folder_cant_escape_the_root() {
    let s = setup();
    s.f.symlink("Library/Caches/sneaky", &s.f.path("Documents"))
        .unwrap();
    // Looks like it's inside the cache; really it's ~/Documents/thesis.docx.
    let path = s.cache.join("sneaky/thesis.docx");
    assert_eq!(
        check(&s, &path, &inside(&s.cache)),
        Err(Refusal::OutsideRuleRoot)
    );
}

#[test]
fn dot_dot_traversal_is_refused() {
    let s = setup();
    for path in [
        s.cache.join("../../Documents/thesis.docx"),
        s.cache.join("app/../../../Documents"),
        s.cache.join("app/.."),
    ] {
        assert!(
            matches!(
                check(&s, &path, &inside(&s.cache)),
                Err(Refusal::Invalid(_))
            ),
            "{}",
            path.display()
        );
    }
    // A lone `.` is dropped when the path is parsed, so it can only name the same folder.
    let v = check(&s, &s.cache.join("./app"), &inside(&s.cache)).unwrap();
    assert_eq!(v.path, s.cache.join("app"));
}

#[test]
fn relative_paths_are_refused() {
    let s = setup();
    assert!(matches!(
        check(&s, Path::new("Library/Caches/app"), &inside(&s.cache)),
        Err(Refusal::Invalid(_))
    ));
}

#[test]
fn the_root_itself_is_refused_unless_the_rule_names_it_exactly() {
    let s = setup();
    assert_eq!(
        check(&s, &s.cache.join("app"), &inside(&s.cache.join("app"))),
        Err(Refusal::OutsideRuleRoot)
    );
    let exact = vec![Root {
        path: s.cache.join("app"),
        inclusive: true,
    }];
    assert!(check(&s, &s.cache.join("app"), &exact).is_ok());
}

#[test]
fn protected_paths_and_their_ancestors_are_refused_even_if_a_rule_claims_them() {
    let s = setup();
    let f = &s.f;
    // A careless rule rooted at the filesystem root.
    let everything = vec![Root {
        path: f.root.clone(),
        inclusive: true,
    }];
    fs::create_dir_all(f.root.join("System/Library")).unwrap();
    fs::create_dir_all(f.root.join("Applications/Mail.app")).unwrap();
    fs::create_dir_all(f.root.join("Library/Logs")).unwrap();
    f.dir("Library/Keychains").unwrap();
    f.dir("Downloads").unwrap();

    let refused = [
        f.home.clone(),
        f.home.parent().unwrap().to_path_buf(), // /Users: an ancestor of home
        f.path("Documents"),
        f.path("Downloads"),
        f.path("Library"),
        f.path(".ssh"),
        f.path(".ssh/id_ed25519"),
        f.path("Library/Keychains"),
        f.root.join("System"),
        f.root.join("System/Library"),
        f.root.join("Applications/Mail.app"),
        f.root.join("Library"),
    ];
    for path in &refused {
        assert_eq!(
            check(&s, path, &everything),
            Err(Refusal::Protected),
            "{}",
            path.display()
        );
    }
    // Inside a top-level folder is fine; the folder itself is not.
    assert!(check(&s, &f.path("Documents/thesis.docx"), &everything).is_ok());
    assert!(check(&s, &f.root.join("Library/Logs"), &everything).is_ok());
}

#[test]
fn git_folders_are_protected() {
    let s = setup();
    s.f.file("Library/Caches/proj/.git/HEAD", 100).unwrap();
    assert_eq!(
        check(&s, &s.cache.join("proj/.git"), &inside(&s.cache)),
        Err(Refusal::Protected)
    );
    assert_eq!(
        check(&s, &s.cache.join("proj/.git/HEAD"), &inside(&s.cache)),
        Err(Refusal::Protected)
    );
}

#[test]
fn a_repository_inside_user_data_is_refused_but_allowed_in_regenerating_caches() {
    let s = setup();
    s.f.file("Library/Caches/checkout/.git/HEAD", 100).unwrap();
    s.f.file("Library/Caches/checkout/src.rs", 100).unwrap();
    let path = s.cache.join("checkout");
    let target = |regenerates| Target {
        path: &path,
        roots: &[],
        snapshot: snapshot(&path),
        regenerates,
        cross_filesystems: false,
        exclude: &[],
        related_apps: &[],
    };
    let roots = inside(&s.cache);
    let mut t = target(false);
    t.roots = &roots;
    assert_eq!(
        s.guard.check(&t, &NoProcesses, &CancelToken::new()),
        Err(Refusal::ContainsRepository)
    );
    let mut t = target(true);
    t.roots = &roots;
    assert!(s.guard.check(&t, &NoProcesses, &CancelToken::new()).is_ok());
}

#[test]
fn jclean_keep_markers_protect_ancestors_and_descendants() {
    let s = setup();
    s.f.file("Library/Caches/kept/.jclean-keep", 0).unwrap();
    s.f.file("Library/Caches/kept/inner/data", 100).unwrap();
    s.f.file("Library/Caches/outer/deep/.jclean-keep", 0)
        .unwrap();
    assert_eq!(
        check(&s, &s.cache.join("kept/inner"), &inside(&s.cache)),
        Err(Refusal::KeepMarker)
    );
    assert_eq!(
        check(&s, &s.cache.join("kept"), &inside(&s.cache)),
        Err(Refusal::KeepMarker)
    );
    assert_eq!(
        check(&s, &s.cache.join("outer"), &inside(&s.cache)),
        Err(Refusal::KeepMarker)
    );
}

#[test]
fn unusual_names_are_handled_exactly() {
    let s = setup();
    let mut names = vec![
        "with space",
        "ünïcödé-ファイル",
        "-leading-dash",
        "$(touch x)",
        "semi;colon",
    ];
    // Control characters can't be in a Windows file name.
    if cfg!(unix) {
        names.extend(["new\nline", "tab\tand;semicolon"]);
    }
    for name in names {
        let rel = format!("Library/Caches/{name}");
        s.f.file(&format!("{rel}/data"), 1_000).unwrap();
        let v = check(&s, &s.f.path(&rel), &inside(&s.cache))
            .unwrap_or_else(|e| panic!("{name:?}: {e}"));
        assert_eq!(v.path.file_name().unwrap().to_string_lossy(), name);
    }
}

#[test]
fn growth_after_the_scan_is_refused() {
    let s = setup();
    let path = s.cache.join("app");
    let snap = snapshot(&path);
    s.f.file("Library/Caches/app/new-download", 500_000)
        .unwrap();
    let roots = inside(&s.cache);
    let t = Target {
        path: &path,
        roots: &roots,
        snapshot: snap,
        regenerates: true,
        cross_filesystems: false,
        exclude: &[],
        related_apps: &[],
    };
    assert_eq!(
        s.guard.check(&t, &NoProcesses, &CancelToken::new()),
        Err(Refusal::ChangedSinceScan)
    );
}

#[test]
fn modification_after_the_scan_is_refused() {
    let s = setup();
    s.f.age("Library/Caches/app", 10).unwrap();
    let path = s.cache.join("app");
    let snap = snapshot(&path);
    // Same size, but the item itself changed.
    filetime::set_file_mtime(&path, filetime::FileTime::now()).unwrap();
    let roots = inside(&s.cache);
    let t = Target {
        path: &path,
        roots: &roots,
        snapshot: snap,
        regenerates: true,
        cross_filesystems: false,
        exclude: &[],
        related_apps: &[],
    };
    assert_eq!(
        s.guard.check(&t, &NoProcesses, &CancelToken::new()),
        Err(Refusal::ChangedSinceScan)
    );
}

#[test]
fn missing_paths_are_reported_as_gone() {
    let s = setup();
    assert_eq!(
        check(&s, &s.cache.join("never-existed"), &inside(&s.cache)),
        Err(Refusal::NotFound)
    );
}

#[test]
fn read_only_files_can_be_verified() {
    let s = setup();
    let file = s.f.file("Library/Caches/ro/file", 1_000).unwrap();
    let mut perms = fs::metadata(&file).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&file, perms).unwrap();
    assert!(check(&s, &s.cache.join("ro"), &inside(&s.cache)).is_ok());
}

#[test]
fn running_related_apps_block_cleaning() {
    let s = setup();
    let path = s.cache.join("app");
    let roots = inside(&s.cache);
    let apps = vec!["Xcode".to_string()];
    let t = Target {
        path: &path,
        roots: &roots,
        snapshot: snapshot(&path),
        regenerates: true,
        cross_filesystems: false,
        exclude: &[],
        related_apps: &apps,
    };
    let running = FakeProcesses(vec!["Xcode".to_string()]);
    assert_eq!(
        s.guard.check(&t, &running, &CancelToken::new()),
        Err(Refusal::AppRunning(vec!["Xcode".to_string()]))
    );
    assert!(
        s.guard
            .check(&t, &FakeProcesses(vec![]), &CancelToken::new())
            .is_ok()
    );
}

// Property: whatever path a buggy rule or scan produces, an approved path is
// strictly inside the rule root, after resolving every symlink in its parents.
fn component() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("..".to_string()),
        Just(".".to_string()),
        Just("to-docs".to_string()),
        Just("to-cache".to_string()),
        Just("to-parent".to_string()),
        Just("sub".to_string()),
        Just("blob".to_string()),
        Just("thesis.docx".to_string()),
        Just("Documents".to_string()),
        "[a-z ]{1,6}",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn the_guard_never_approves_a_path_outside_the_rule_roots(parts in prop::collection::vec(component(), 1..7), from_home in any::<bool>()) {
        let s = setup();
        s.f.file("Library/Caches/app/sub/blob", 10).unwrap();
        s.f.symlink("Library/Caches/app/to-docs", &s.f.path("Documents")).unwrap();
        s.f.symlink("Library/Caches/app/sub/to-cache", &s.cache).unwrap();
        s.f.symlink("Library/Caches/app/to-parent", &s.f.path("Library")).unwrap();
        let root = s.cache.join("app");
        let base = if from_home { s.f.home.clone() } else { root.clone() };
        let path = parts.iter().fold(base, |p, c| p.join(c));

        if let Ok(canonical) = s.guard.check_path(&path, &inside(&root)) {
            let canonical_root = jclean_core::platform::fs::canonicalize(&root).unwrap();
            prop_assert!(canonical.starts_with(&canonical_root) && canonical != canonical_root, "{} approved", path.display());
            let resolved_parent = jclean_core::platform::fs::canonicalize(canonical.parent().unwrap()).unwrap();
            prop_assert!(resolved_parent.starts_with(&canonical_root), "{} resolves outside", path.display());
        }
    }
}
