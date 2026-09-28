//! End-to-end: scan a fixture home, plan, and clean (dry run and for real).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use common::{FakeProcesses, FakeRunner, FakeTrash};
use jclean_core::cancel::CancelToken;
use jclean_core::cleaner::{self, CleanContext, CleanReport, Outcome};
use jclean_core::env::Os;
use jclean_core::history::History;
use jclean_core::planner::{CleanPlan, Selection, build_plan};
use jclean_core::rules::{Audience, Method, RuleSet};
use jclean_core::safety::{NoProcesses, ProcessChecker, SafetyGuard};
use jclean_core::scanner::{ScanMode, ScanOptions, ScanResult, Scanner};
use jclean_core::sizing::{InodeSet, MeasureOptions, measure};
use jclean_core::testing::Fixture;

struct World {
    tmp: tempfile::TempDir,
    f: Fixture,
    rules: RuleSet,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let f = Fixture::standard(tmp.path()).unwrap();
    World {
        rules: RuleSet::builtin(Os::Macos).unwrap(),
        tmp,
        f,
    }
}

fn scan(w: &World, runner: &FakeRunner) -> ScanResult {
    let env = w.f.env();
    let scanner = Scanner {
        env: &env,
        rules: &w.rules,
        runner,
    };
    let mut opts = ScanOptions::new(ScanMode::Quick, Audience::Developer);
    opts.run_probes = false;
    scanner.scan(&opts, &CancelToken::new(), &|_| {})
}

fn plan(w: &World, scan: &ScanResult, selection: &Selection, runner: &FakeRunner) -> CleanPlan {
    build_plan(scan, &w.rules, &selection.resolve(scan), &w.f.env(), runner)
}

fn run(
    w: &World,
    plan: &CleanPlan,
    runner: &FakeRunner,
    processes: &dyn ProcessChecker,
    history: Option<&History>,
    dry_run: bool,
) -> CleanReport {
    let trash = FakeTrash::new(w.tmp.path().join("fake-trash"));
    let guard = SafetyGuard::new(&w.f.env());
    let ctx = CleanContext {
        guard: &guard,
        runner,
        trasher: &trash,
        processes,
        history,
        scan_id: None,
        dry_run,
    };
    cleaner::execute(plan, &ctx, &CancelToken::new(), &|_| {})
}

fn id(rule: &str, path: &Path) -> String {
    format!("{rule}:{}", path.display())
}

fn size(path: &Path) -> u64 {
    measure(
        path,
        &MeasureOptions::default(),
        &InodeSet::new(),
        &CancelToken::new(),
    )
    .map_or(0, |m| m.allocated)
}

#[test]
fn dry_run_changes_nothing_and_logs_every_item() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let plan = plan(&w, &result, &Selection::AllSafe, &runner);
    assert!(!plan.items.is_empty());
    let before = size(&w.f.home);

    let history = History::open_in_memory().unwrap();
    let report = run(&w, &plan, &runner, &NoProcesses, Some(&history), true);

    assert_eq!(
        size(&w.f.home),
        before,
        "a dry run must not change anything"
    );
    assert!(report.outcomes.iter().all(|o| matches!(
        o.outcome,
        Outcome::WouldClean { .. } | Outcome::Skipped { .. }
    )));
    let logged = history.actions(report.cleanup_id).unwrap();
    assert_eq!(logged.len(), plan.items.len());
    assert!(logged.iter().any(|a| a.outcome == "dry-run"));
    assert!(history.cleanups().unwrap()[0].dry_run);
}

#[test]
fn cleaning_preselected_items_frees_their_space_and_nothing_else() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let plan = plan(&w, &result, &Selection::Preselected, &runner);

    let selected: Vec<&str> = plan.items.iter().map(|i| i.item_id.as_str()).collect();
    for expected in [
        id("macos.node.npm-cache", &w.f.path(".npm/_cacache")),
        id(
            "macos.node.project-dependencies",
            &w.f.path("code/old-site/node_modules"),
        ),
        id("macos.rust.project-target", &w.f.path("code/rusty/target")),
    ] {
        assert!(
            selected.contains(&expected.as_str()),
            "{expected} should be pre-selected"
        );
    }
    for never in [
        id(
            "macos.node.project-dependencies",
            &w.f.path("code/active-app/node_modules"),
        ),
        id(
            "macos.downloads.old-installers",
            &w.f.path("Downloads/Setup.dmg"),
        ),
        id(
            "macos.system.app-caches",
            &w.f.path("Library/Caches/com.example.keepme"),
        ),
    ] {
        assert!(
            !selected.contains(&never.as_str()),
            "{never} must not be pre-selected"
        );
    }

    let delete_estimate: u64 = plan
        .items
        .iter()
        .filter(|i| i.method == Method::Delete)
        .map(|i| i.bytes)
        .sum();
    let before = size(&w.f.home);
    let history = History::open_in_memory().unwrap();
    let report = run(&w, &plan, &runner, &NoProcesses, Some(&history), false);
    let after = size(&w.f.home);

    assert_eq!(report.failed, 0, "{:#?}", report.outcomes);
    assert!(!w.f.path(".npm/_cacache").exists());
    assert!(!w.f.path("code/old-site/node_modules").exists());
    assert!(!w.f.path("code/rusty/target").exists());
    // Everything around them is untouched.
    assert!(w.f.path("Documents/important.txt").exists());
    assert!(w.f.path("code/old-site/package.json").exists());
    assert!(w.f.path("code/active-app/node_modules").exists());
    assert!(
        w.f.path("Library/Caches/com.example.keepme/data.bin")
            .exists()
    );
    assert!(w.f.path("Downloads/Setup.dmg").exists());

    // Phase 4 bar, checked early: freed space matches the estimate within 5% for deletes.
    let freed = before - after;
    assert!(
        freed.abs_diff(delete_estimate) * 20 <= delete_estimate,
        "freed {freed}, estimated {delete_estimate}"
    );
    assert!(
        history
            .actions(report.cleanup_id)
            .unwrap()
            .iter()
            .all(|a| a.outcome == "cleaned")
    );
}

#[test]
fn a_symlink_in_a_cache_is_removed_as_a_link() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let editor = id(
        "macos.system.app-caches",
        &w.f.path("Library/Caches/com.example.editor"),
    );
    let plan = plan(&w, &result, &Selection::Ids(vec![editor]), &runner);
    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert_eq!(report.failed, 0, "{:#?}", report.outcomes);
    assert!(!w.f.path("Library/Caches/com.example.editor").exists());
    // The link pointed at ~/Documents, which must be intact.
    assert!(w.f.path("Documents/important.txt").exists());
}

#[test]
fn keep_root_clears_contents_but_keeps_the_folder() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let crash = id(
        "macos.system.crash-reports",
        &w.f.path("Library/Logs/DiagnosticReports"),
    );
    let plan = plan(&w, &result, &Selection::Ids(vec![crash]), &runner);
    run(&w, &plan, &runner, &NoProcesses, None, false);
    assert!(w.f.path("Library/Logs/DiagnosticReports").is_dir());
    assert!(
        !w.f.path("Library/Logs/DiagnosticReports/Thing.crash")
            .exists()
    );
}

#[test]
fn cleaning_an_outer_item_leaves_nested_items_alone() {
    let w = world();
    w.f.file("Library/Caches/Google/Chrome/Default/Cache/data", 1_000_000)
        .unwrap();
    w.f.file("Library/Caches/Google/Other/thing", 500_000)
        .unwrap();
    w.f.age("Library/Caches/Google", 30).unwrap();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let google = id(
        "macos.system.app-caches",
        &w.f.path("Library/Caches/Google"),
    );
    let item = result.item(&google).unwrap();
    assert!(
        item.bytes < 700_000,
        "Chrome's cache belongs to the Chrome rule: {}",
        item.bytes
    );

    let plan = plan(&w, &result, &Selection::Ids(vec![google]), &runner);
    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert_eq!(report.failed, 0, "{:#?}", report.outcomes);
    assert!(!w.f.path("Library/Caches/Google/Other").exists());
    assert!(
        w.f.path("Library/Caches/Google/Chrome/Default/Cache/data")
            .exists()
    );
}

#[test]
fn trash_items_go_to_the_trash_and_are_reported() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let dmg = id(
        "macos.downloads.old-installers",
        &w.f.path("Downloads/Setup.dmg"),
    );
    let plan = plan(&w, &result, &Selection::Ids(vec![dmg]), &runner);
    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert!(!w.f.path("Downloads/Setup.dmg").exists());
    assert!(report.trashed_bytes > 3_000_000);
    assert!(w.tmp.path().join("fake-trash/0-Setup.dmg").exists());
}

#[test]
fn items_changed_since_the_scan_are_skipped() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let npm = id("macos.node.npm-cache", &w.f.path(".npm/_cacache"));
    let plan = plan(&w, &result, &Selection::Ids(vec![npm]), &runner);
    w.f.file(".npm/_cacache/content-v2/sha512/bb/new", 2_000_000)
        .unwrap();

    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert_eq!(
        report.outcomes[0].outcome,
        Outcome::Skipped {
            reason: "Changed since scan".to_string()
        }
    );
    assert!(w.f.path(".npm/_cacache").exists());
}

#[test]
fn a_running_related_app_pauses_that_item_only() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let derived = id(
        "macos.xcode.derived-data",
        &w.f.path("Library/Developer/Xcode/DerivedData/MyApp-abc123"),
    );
    let target = id("macos.rust.project-target", &w.f.path("code/rusty/target"));
    let plan = plan(
        &w,
        &result,
        &Selection::Ids(vec![derived.clone(), target]),
        &runner,
    );
    assert_eq!(plan.related_apps, vec!["Xcode".to_string()]);

    let report = run(
        &w,
        &plan,
        &runner,
        &FakeProcesses(vec!["Xcode".to_string()]),
        None,
        false,
    );
    let xcode = report
        .outcomes
        .iter()
        .find(|o| o.item_id == derived)
        .unwrap();
    assert_eq!(
        xcode.outcome,
        Outcome::Skipped {
            reason: "Close Xcode first".to_string()
        }
    );
    assert!(
        w.f.path("Library/Developer/Xcode/DerivedData/MyApp-abc123")
            .exists()
    );
    // Failures never stop the rest of the plan (spec §9, step 6).
    assert!(!w.f.path("code/rusty/target").exists());
}

#[test]
fn tool_commands_run_once_without_a_shell_and_not_in_dry_runs() {
    let w = world();
    let runner = FakeRunner::none().with_tool("npm", "");
    let result = scan(&w, &runner);
    let npm = id("macos.node.npm-cache", &w.f.path(".npm/_cacache"));
    let plan = plan(&w, &result, &Selection::Ids(vec![npm]), &runner);
    assert_eq!(plan.items[0].method, Method::Command);
    assert_eq!(
        plan.items[0].command.as_ref().unwrap().display(),
        "npm cache clean --force"
    );

    run(&w, &plan, &runner, &NoProcesses, None, true);
    assert!(runner.calls().is_empty(), "dry runs never run tools");

    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert!(matches!(
        report.outcomes[0].outcome,
        Outcome::Cleaned { .. }
    ));
    assert_eq!(
        runner.calls(),
        vec![(
            Path::new("/fake/bin/npm").to_path_buf(),
            vec!["cache".into(), "clean".into(), "--force".into()]
        )]
    );
}

#[test]
fn item_names_that_look_like_flags_are_never_passed_to_tools() {
    let w = world();
    w.f.file("Library/Developer/CoreSimulator/Devices/--all/data", 10_000)
        .unwrap();
    let runner = FakeRunner::none().with_tool("xcrun", "");
    let result = scan(&w, &runner);
    let device = id(
        "macos.simulator.devices",
        &w.f.path("Library/Developer/CoreSimulator/Devices/--all"),
    );
    let plan = plan(&w, &result, &Selection::Ids(vec![device]), &runner);
    assert!(plan.items.is_empty());
    assert_eq!(plan.skipped.len(), 1);
}

#[test]
fn blocked_and_unknown_items_are_skipped_with_a_reason() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let keepme = id(
        "macos.system.app-caches",
        &w.f.path("Library/Caches/com.example.keepme"),
    );
    let plan = plan(
        &w,
        &result,
        &Selection::Ids(vec![keepme, "nope".to_string()]),
        &runner,
    );
    assert!(plan.items.is_empty());
    let reasons: Vec<&str> = plan.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(
        reasons,
        vec![
            "It's marked to keep with a .jclean-keep file",
            "Not found in this scan"
        ]
    );
}

#[test]
fn read_only_files_are_removed() {
    let w = world();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let editor = id(
        "macos.system.app-caches",
        &w.f.path("Library/Caches/com.example.editor"),
    );
    let plan = plan(&w, &result, &Selection::Ids(vec![editor]), &runner);
    let report = run(&w, &plan, &runner, &NoProcesses, None, false);
    assert_eq!(report.failed, 0);
    assert!(
        !w.f.path("Library/Caches/com.example.editor/readonly.bin")
            .exists()
    );
}

#[test]
fn deleting_user_files_is_opt_in_and_never_applies_to_caution_items() {
    use jclean_core::planner::{PlanOptions, build_plan_with};
    let w = world();
    w.f.file(
        "Library/Application Support/MobileSync/Backup/abc123/Manifest.db",
        50_000,
    )
    .unwrap();
    w.f.age("Library/Application Support/MobileSync", 100)
        .unwrap();
    let runner = FakeRunner::none();
    let result = scan(&w, &runner);
    let dmg = id(
        "macos.downloads.old-installers",
        &w.f.path("Downloads/Setup.dmg"),
    );
    let backup = id(
        "macos.mobile.device-backups",
        &w.f.path("Library/Application Support/MobileSync/Backup/abc123"),
    );
    let ids = vec![dmg.clone(), backup.clone()];
    let method = |plan: &CleanPlan, id: &str| {
        plan.items
            .iter()
            .find(|i| i.item_id == id)
            .map(|i| i.method)
    };

    let default = build_plan_with(
        &result,
        &w.rules,
        &ids,
        &w.f.env(),
        &runner,
        PlanOptions::default(),
    );
    assert_eq!(method(&default, &dmg), Some(Method::Trash));

    let delete = build_plan_with(
        &result,
        &w.rules,
        &ids,
        &w.f.env(),
        &runner,
        PlanOptions {
            delete_user_files: true,
        },
    );
    assert_eq!(method(&delete, &dmg), Some(Method::Delete));
    assert_eq!(
        method(&delete, &backup),
        Some(Method::Trash),
        "caution items always go to the Trash"
    );
    assert!(delete.needs_second_confirmation);
}
