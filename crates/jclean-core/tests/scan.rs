//! Sizing and scanning against fixture homes (spec §4, §16). Phase 1
//! acceptance: quick scan lists the expected items; sizes match `du -k`
//! within 2%.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::process::Command;

use common::FakeRunner;
use jclean_core::cancel::CancelToken;
use jclean_core::env::Os;
use jclean_core::rules::{self, Audience, Method, Risk, RuleSet, RuleSource};
use jclean_core::safety::Refusal;
use jclean_core::scanner::{Blocked, ScanItem, ScanMode, ScanOptions, ScanResult, Scanner};
use jclean_core::sizing::{InodeSet, MeasureOptions, measure};
use jclean_core::testing::Fixture;

fn scan(
    f: &Fixture,
    rules: &RuleSet,
    mode: ScanMode,
    audience: Audience,
    runner: &FakeRunner,
) -> ScanResult {
    let env = f.env();
    let scanner = Scanner {
        env: &env,
        rules,
        runner,
    };
    let mut opts = ScanOptions::new(mode, audience);
    opts.run_probes = false;
    scanner.scan(&opts, &CancelToken::new(), &|_| {})
}

fn find<'a>(result: &'a ScanResult, rule: &str, path: &Path) -> &'a ScanItem {
    let id = format!("{rule}:{}", path.display());
    result.item(&id).unwrap_or_else(|| {
        let ids: Vec<_> = result.items.iter().map(|i| i.id.as_str()).collect();
        panic!("missing {id}\nfound: {ids:#?}")
    })
}

fn has(result: &ScanResult, rule: &str, path: &Path) -> bool {
    result.item(&format!("{rule}:{}", path.display())).is_some()
}

/// `du -sk` in bytes (1 KiB blocks).
fn du(path: &Path) -> u64 {
    let out = Command::new("du")
        .arg("-sk")
        .arg(path)
        .output()
        .expect("du runs");
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 1024
}

fn within_2_percent(ours: u64, theirs: u64) -> bool {
    ours.abs_diff(theirs) * 50 <= theirs.max(1)
}

#[test]
fn sizes_match_du_within_two_percent() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    for rel in [
        "",
        ".npm",
        "Library",
        "code",
        "code/old-site/node_modules",
        "Library/Caches/com.example.editor",
    ] {
        let path = f.path(rel);
        let ours = measure(
            &path,
            &MeasureOptions::default(),
            &InodeSet::new(),
            &CancelToken::new(),
        )
        .unwrap()
        .allocated;
        let theirs = du(&path);
        assert!(
            within_2_percent(ours, theirs),
            "{rel:?}: ours {ours}, du {theirs}"
        );
    }
}

#[test]
fn hard_links_count_once_and_symlinks_are_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::new(tmp.path()).unwrap();
    f.file("a/big", 2_000_000).unwrap();
    f.hard_link("a/big", "a/also-big").unwrap();
    f.file("elsewhere/huge", 5_000_000).unwrap();
    f.symlink("a/link", &f.path("elsewhere")).unwrap();

    let m = measure(
        &f.path("a"),
        &MeasureOptions::default(),
        &InodeSet::new(),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(m.files, 3);
    assert!(
        m.allocated >= 2_000_000 && m.allocated < 2_200_000,
        "{}",
        m.allocated
    );

    // The link itself is measured, never its target.
    let link = measure(
        &f.path("a/link"),
        &MeasureOptions::default(),
        &InodeSet::new(),
        &CancelToken::new(),
    )
    .unwrap();
    assert!(link.is_symlink);
    assert!(link.allocated < 100_000);
}

#[test]
fn excluded_paths_are_left_out() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::new(tmp.path()).unwrap();
    f.file("c/keep/one", 1_000_000).unwrap();
    f.file("c/claimed/two", 3_000_000).unwrap();
    let opts = MeasureOptions {
        cross_filesystems: false,
        exclude: [f.path("c/claimed")].into_iter().collect(),
    };
    let m = measure(&f.path("c"), &opts, &InodeSet::new(), &CancelToken::new()).unwrap();
    assert!(m.allocated < 1_200_000, "{}", m.allocated);
}

#[test]
fn quick_scan_lists_expected_items() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    let rules = RuleSet::builtin(Os::Macos).unwrap();
    let result = scan(
        &f,
        &rules,
        ScanMode::Quick,
        Audience::Developer,
        &FakeRunner::none(),
    );
    assert!(!result.cancelled);

    // npm: tool not installed, so the rule falls back to deleting.
    let npm = find(&result, "macos.node.npm-cache", &f.path(".npm/_cacache"));
    assert_eq!(npm.method, Method::Delete);
    assert!(npm.cleanable && npm.preselected);
    assert!(npm.bytes >= 3_000_000);

    // The specific Yarn rule wins over the general app-caches rule (spec §6.3).
    find(
        &result,
        "macos.node.yarn-cache",
        &f.path("Library/Caches/Yarn"),
    );
    assert!(!has(
        &result,
        "macos.system.app-caches",
        &f.path("Library/Caches/Yarn")
    ));
    // Apple's own caches are never offered as app caches.
    assert!(!has(
        &result,
        "macos.system.app-caches",
        &f.path("Library/Caches/com.apple.Safari")
    ));

    // The symlink inside a cache is measured as a link, not as ~/Documents.
    let editor = find(
        &result,
        "macos.system.app-caches",
        &f.path("Library/Caches/com.example.editor"),
    );
    assert!(editor.bytes < 1_400_000, "{}", editor.bytes);

    // A .jclean-keep marker blocks the folder.
    let kept = find(
        &result,
        "macos.system.app-caches",
        &f.path("Library/Caches/com.example.keepme"),
    );
    assert!(!kept.cleanable && !kept.preselected);
    assert_eq!(
        kept.blocked,
        Some(Blocked::Safety {
            reason: Refusal::KeepMarker
        })
    );

    // Logs: crash reports are their own item, not part of the general logs item.
    find(
        &result,
        "macos.system.crash-reports",
        &f.path("Library/Logs/DiagnosticReports"),
    );
    find(
        &result,
        "macos.system.app-logs",
        &f.path("Library/Logs/SomeApp"),
    );
    assert!(!has(
        &result,
        "macos.system.app-logs",
        &f.path("Library/Logs/DiagnosticReports")
    ));

    find(
        &result,
        "macos.xcode.derived-data",
        &f.path("Library/Developer/Xcode/DerivedData/MyApp-abc123"),
    );
    find(
        &result,
        "macos.system.trash",
        &f.path(".Trash/old-photo.jpg"),
    );

    // Only installers older than 30 days.
    let dmg = find(
        &result,
        "macos.downloads.old-installers",
        &f.path("Downloads/Setup.dmg"),
    );
    assert_eq!(dmg.method, Method::Trash);
    assert!(!dmg.preselected, "review items are never pre-selected");
    assert!(!has(
        &result,
        "macos.downloads.old-installers",
        &f.path("Downloads/fresh.pkg")
    ));
    assert!(!has(
        &result,
        "macos.downloads.old-installers",
        &f.path("Downloads/notes.txt")
    ));

    // pnpm isn't installed and has no fallback.
    let pnpm = find(
        &result,
        "macos.node.pnpm-store",
        &f.path("Library/pnpm/store"),
    );
    assert!(!pnpm.cleanable);
    assert!(matches!(pnpm.blocked, Some(Blocked::ToolMissing { .. })));

    // Projects: inactive ones are safe and pre-selected, active ones need review.
    let old = find(
        &result,
        "macos.node.project-dependencies",
        &f.path("code/old-site/node_modules"),
    );
    assert_eq!((old.risk, old.preselected), (Risk::Safe, true));
    assert!(!old.project.as_ref().unwrap().active);
    let active = find(
        &result,
        "macos.node.project-dependencies",
        &f.path("code/active-app/node_modules"),
    );
    assert_eq!((active.risk, active.preselected), (Risk::Review, false));
    // Old package files, but the repository was used today.
    let mono = find(
        &result,
        "macos.node.project-dependencies",
        &f.path("code/mono/packages/web/node_modules"),
    );
    assert_eq!(mono.risk, Risk::Review);
    let dist = find(
        &result,
        "macos.node.project-output",
        &f.path("code/old-site/dist"),
    );
    assert_eq!(dist.risk, Risk::Review);
    let target = find(
        &result,
        "macos.rust.project-target",
        &f.path("code/rusty/target"),
    );
    assert!(target.preselected);

    // The hard-linked pnpm file is counted once across the whole scan.
    let linked = pnpm.bytes + active.bytes;
    assert!(linked < 5_000_000 + 2_000_000 + 400_000, "{linked}");

    // Nothing outside the fixture shows up.
    for item in &result.items {
        if let Some(path) = &item.path {
            assert!(
                path.starts_with(&f.root),
                "{} escaped the fixture",
                path.display()
            );
        }
    }
}

#[test]
fn everyday_mode_shows_only_everyday_items() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    let rules = RuleSet::builtin(Os::Macos).unwrap();
    let result = scan(
        &f,
        &rules,
        ScanMode::Quick,
        Audience::Everyday,
        &FakeRunner::none(),
    );
    assert!(
        result
            .items
            .iter()
            .any(|i| i.rule_id == "macos.system.app-caches")
    );
    assert!(
        result
            .items
            .iter()
            .all(|i| !i.rule_id.starts_with("macos.node."))
    );
    assert!(
        result
            .items
            .iter()
            .all(|i| i.rule_id != "macos.xcode.derived-data")
    );
    assert!(result.projects.is_empty());
}

#[test]
fn full_scan_maps_the_disk_and_finds_unclaimed_large_files() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    f.file("Movies/old-render.mov", 5_000_000).unwrap();
    f.file("Library/Application Support/SomeApp/huge.db", 5_000_000)
        .unwrap();
    f.age("Movies/old-render.mov", 200).unwrap();
    f.age("Library/Application Support/SomeApp/huge.db", 200)
        .unwrap();

    // The built-in large-file rule, scaled down to fixture sizes.
    let mut rules = RuleSet::builtin(Os::Macos).unwrap();
    let json = r#"[{"id":"macos.test.large","version":1,"platforms":["macos"],"audience":["everyday"],"category":"documents",
        "group":"Large files","labels":{"developer":"Large","everyday":"Large"},"description":{"what":"w","ifCleared":"i"},
        "icon":"file","risk":"review","regenerates":false,
        "detect":{"kind":"query","paths":["{home}"],"minBytes":3500000,"olderThanDays":90,"recursive":true},
        "cleanup":{"method":"trash"}}]"#;
    rules
        .extend(rules::parse_file("test.json", json, RuleSource::Builtin).unwrap())
        .unwrap();

    let result = scan(
        &f,
        &rules,
        ScanMode::Full,
        Audience::Developer,
        &FakeRunner::none(),
    );
    let tree = &result.tree[0];
    assert!(tree.allocated > 20_000_000);
    assert!(
        within_2_percent(tree.allocated, du(&f.home)),
        "tree {} vs du {}",
        tree.allocated,
        du(&f.home)
    );

    find(
        &result,
        "macos.test.large",
        &f.path("Movies/old-render.mov"),
    );
    // Claimed by another rule, inside ~/Library, or inside a project folder: not a large file.
    assert!(!has(
        &result,
        "macos.test.large",
        &f.path("Downloads/Setup.dmg")
    ));
    assert!(!has(
        &result,
        "macos.test.large",
        &f.path("Library/Application Support/SomeApp/huge.db")
    ));
    assert!(!has(
        &result,
        "macos.test.large",
        &f.path("code/old-site/node_modules/react/index.js")
    ));
    assert!(has(
        &result,
        "macos.downloads.old-installers",
        &f.path("Downloads/Setup.dmg")
    ));

    // The folder map: one level at a time, with what's reclaimable inside each folder.
    let env = f.env();
    let top = jclean_core::map::children(&result, &env, "").expect("top level");
    let code = top.iter().find(|c| c.name == "code").expect("code folder");
    assert!(code.has_children && code.reclaimable > 5_000_000);
    let library = top
        .iter()
        .find(|c| c.name == "Library")
        .expect("Library folder");
    assert_eq!(library.category, rules::Category::System);
    let inside = jclean_core::map::children(&result, &env, &code.id).expect("inside code");
    let old_site = inside
        .iter()
        .find(|c| c.name == "old-site")
        .expect("project folder");
    assert!(old_site.reclaimable > 0);
    let nm = jclean_core::map::children(&result, &env, &old_site.id)
        .expect("inside the project")
        .into_iter()
        .find(|c| c.name == "node_modules")
        .expect("node_modules cell");
    assert!(
        nm.item_id.is_some(),
        "a cell that is exactly one item links to it"
    );
    assert!(jclean_core::map::children(&result, &env, "fs:/nowhere").is_none());
}

#[cfg(unix)]
#[test]
fn locations_macos_keeps_from_us_are_reported_not_hidden() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    let trash = f.path(".Trash");
    // What missing Full Disk Access looks like: the folder exists but can't be listed.
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o000)).unwrap();
    let rules = RuleSet::builtin(Os::Macos).unwrap();
    let result = scan(
        &f,
        &rules,
        ScanMode::Quick,
        Audience::Everyday,
        &FakeRunner::none(),
    );
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        result
            .needs_access
            .contains(&"macos.system.trash".to_string()),
        "{:?}",
        result.needs_access
    );
    assert!(
        !result
            .needs_access
            .contains(&"macos.system.app-caches".to_string())
    );
}

#[test]
fn cancelled_scans_return_partial_results() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    let rules = RuleSet::builtin(Os::Macos).unwrap();
    let env = f.env();
    let runner = FakeRunner::none();
    let scanner = Scanner {
        env: &env,
        rules: &rules,
        runner: &runner,
    };
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = scanner.scan(
        &ScanOptions::new(ScanMode::Quick, Audience::Developer),
        &cancel,
        &|_| {},
    );
    assert!(result.cancelled);
}

#[test]
fn probes_report_tool_managed_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::new(tmp.path()).unwrap();
    let rules = RuleSet::builtin(Os::Macos).unwrap();
    let docker_df = concat!(
        r#"{"Type":"Images","TotalCount":"5","Active":"2","Size":"2.3GB","Reclaimable":"1.1GB (47%)"}"#,
        "\n",
        r#"{"Type":"Build Cache","TotalCount":"9","Active":"0","Size":"800MB","Reclaimable":"800MB"}"#,
        "\n",
        r#"{"Type":"Local Volumes","TotalCount":"1","Active":"1","Size":"10MB","Reclaimable":"0B (0%)"}"#
    );
    let runner = FakeRunner::none().with_tool("docker", docker_df).with_tool(
        "tmutil",
        "Snapshots for disk /:\ncom.apple.TimeMachine.2024-05-01-123456.local\n",
    );
    let env = f.env();
    let scanner = Scanner {
        env: &env,
        rules: &rules,
        runner: &runner,
    };
    let result = scanner.scan(
        &ScanOptions::new(ScanMode::Quick, Audience::Developer),
        &CancelToken::new(),
        &|_| {},
    );

    let images = result.item("macos.docker.unused-images:images").unwrap();
    assert_eq!(images.bytes, 1_100_000_000);
    assert_eq!(images.method, Method::Command);
    assert!(!images.preselected);
    assert_eq!(
        result
            .item("macos.docker.build-cache:build-cache")
            .unwrap()
            .bytes,
        800_000_000
    );
    assert!(result.item("macos.docker.unused-volumes:volumes").is_none());
    let snap = result
        .item("macos.system.local-snapshots:2024-05-01-123456")
        .unwrap();
    assert!(!snap.bytes_known);
    // xcrun isn't installed: reported as a note, not a failure.
    assert!(
        result
            .notes
            .iter()
            .any(|n| n.rule_id.as_deref() == Some("macos.simulator.unavailable"))
    );
}
