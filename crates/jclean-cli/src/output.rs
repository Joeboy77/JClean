//! Human-readable output for the harness. Sizes are decimal, like Finder.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jclean_core::cleaner::{CleanEvent, CleanReport, Outcome};
use jclean_core::env::Env;
use jclean_core::planner::CleanPlan;
use jclean_core::rules::{Audience, Method, Risk, RuleSet};
use jclean_core::scanner::{ScanEvent, ScanItem, ScanResult};
use jclean_core::time::{DAY_SECS, now_secs};
use jclean_core::units::format_bytes;

static LAST_PERCENT: AtomicUsize = AtomicUsize::new(usize::MAX);

pub fn progress(event: &ScanEvent<'_>) {
    let mut err = std::io::stderr();
    match event {
        ScanEvent::Stage(stage) => {
            let _ = write!(err, "\r\x1b[2K{stage}…");
        }
        ScanEvent::Progress { done, total } if *total > 0 => {
            let percent = done * 100 / total;
            if LAST_PERCENT.swap(percent, Ordering::Relaxed) != percent {
                let _ = write!(err, "\r\x1b[2KMeasuring {percent}%");
            }
        }
        ScanEvent::Progress { .. } | ScanEvent::Item(_) => {}
    }
    let _ = err.flush();
}

fn clear_progress() {
    eprint!("\r\x1b[2K");
}

fn tilde(path: &Path, env: &Env) -> String {
    match path.strip_prefix(env.home()) {
        Ok(rel) if rel.as_os_str().is_empty() => "~".to_string(),
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// "Last used 7 months ago" style (spec §4.4).
fn ago(t: i64) -> String {
    let days = (now_secs() - t).max(0) / DAY_SECS;
    match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=59 => format!("{days} days ago"),
        60..=729 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    }
}

fn label(item: &ScanItem, rules: &RuleSet, audience: Audience) -> String {
    let base = rules
        .get(&item.rule_id)
        .map_or(item.rule_id.as_str(), |r| match audience {
            Audience::Developer => r.labels.developer.as_str(),
            Audience::Everyday => r.labels.everyday.as_str(),
        });
    match &item.name {
        Some(name) => format!("{base} · {name}"),
        None => base.to_string(),
    }
}

fn size(item: &ScanItem) -> String {
    if !item.bytes_known {
        "size unknown".to_string()
    } else if item.may_share_blocks {
        format!("up to {}", format_bytes(item.bytes))
    } else {
        format_bytes(item.bytes)
    }
}

/// A list section: which items belong in it.
type Section = dyn Fn(&ScanItem) -> bool;

pub fn scan(result: &ScanResult, rules: &RuleSet, env: &Env, audience: Audience, took: Duration) {
    clear_progress();
    let mode = match audience {
        Audience::Developer => "developer",
        Audience::Everyday => "everyday",
    };
    let preselected: u64 = result
        .items
        .iter()
        .filter(|i| i.preselected)
        .map(|i| i.bytes)
        .sum();
    println!(
        "{:?} scan of {} · {mode} mode · {:.1} s{}",
        result.mode,
        tilde(env.home(), env),
        took.as_secs_f64(),
        if result.cancelled {
            " · cancelled, results are partial"
        } else {
            ""
        }
    );
    println!(
        "{} can be freed across {} items. {} is pre-selected.\n",
        format_bytes(result.reclaimable()),
        result.items.iter().filter(|i| i.cleanable).count(),
        format_bytes(preselected)
    );

    let sections: [(&str, &Section); 5] = [
        ("Safe to clean", &|i| i.cleanable && i.risk == Risk::Safe),
        ("Needs review", &|i| i.cleanable && i.risk == Risk::Review),
        ("Caution", &|i| i.cleanable && i.risk == Risk::Caution),
        ("For your information", &|i| i.risk == Risk::Info),
        ("Can't be cleaned", &|i| {
            !i.cleanable && i.risk != Risk::Info
        }),
    ];
    for (title, belongs) in sections {
        let items: Vec<&ScanItem> = result.items.iter().filter(|i| belongs(i)).collect();
        if items.is_empty() {
            continue;
        }
        let total: u64 = items.iter().map(|i| i.bytes).sum();
        println!("{title} ({})  {}", items.len(), format_bytes(total));
        for item in items {
            let mark = if item.preselected { "[x]" } else { "[ ]" };
            let mut detail = Vec::new();
            if let Some(p) = &item.path {
                detail.push(tilde(p, env));
            }
            if let Some(t) = item.last_used {
                detail.push(format!("last used {}", ago(t)));
            }
            if let Some(project) = &item.project {
                detail.push(if project.active {
                    "active project".to_string()
                } else {
                    "inactive project".to_string()
                });
            }
            if item.method != Method::None {
                detail.push(method_name(item.method).to_string());
            }
            if let Some(b) = &item.blocked {
                detail.push(b.message());
            }
            println!(
                "  {mark} {:<58} {:>13}",
                truncate(&label(item, rules, audience), 58),
                size(item)
            );
            println!("      {}", detail.join(" · "));
        }
        println!();
    }
    for note in &result.notes {
        println!(
            "Note: {}{}",
            note.rule_id
                .as_deref()
                .map(|r| format!("{r}: "))
                .unwrap_or_default(),
            note.message
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn method_name(m: Method) -> &'static str {
    match m {
        Method::Delete => "deleted permanently",
        Method::Trash => "moved to Trash",
        Method::Command => "cleared by its tool",
        Method::None => "not cleanable",
    }
}

pub fn rules(set: &RuleSet) {
    for r in set.rules() {
        println!(
            "{:<44} {:<8} {:<8} {}",
            r.id,
            r.risk.as_str(),
            r.cleanup.method.as_str(),
            r.labels.developer
        );
    }
    println!("\n{} rules", set.rules().len());
}

pub fn plan(plan: &CleanPlan, env: &Env, dry_run: bool) {
    clear_progress();
    println!(
        "Clean plan{}: {} items, {}",
        if dry_run { " (dry run)" } else { "" },
        plan.items.len(),
        format_bytes(plan.total_bytes)
    );
    for m in &plan.by_method {
        println!(
            "  {}: {} items, {}",
            capitalize(method_name(m.method)),
            m.items,
            format_bytes(m.bytes)
        );
    }
    if !plan.related_apps.is_empty() {
        println!("  Close first: {}", plan.related_apps.join(", "));
    }
    if plan.needs_second_confirmation {
        println!("  Includes caution items: the app asks for a second confirmation.");
    }
    println!();
    for item in &plan.items {
        let target = match (&item.command, &item.path) {
            (Some(cmd), _) => cmd.display(),
            (None, Some(p)) => tilde(p, env),
            (None, None) => String::new(),
        };
        println!(
            "  {:>10}  {:<48} {}",
            format_bytes(item.bytes),
            truncate(&item.label, 48),
            target
        );
    }
    for s in &plan.skipped {
        println!("  skipped     {} ({})", s.item_id, s.reason);
    }
    println!();
}

pub fn clean_progress(event: &CleanEvent<'_>) {
    if let CleanEvent::ItemDone(o) = event {
        let status = match &o.outcome {
            Outcome::Cleaned { bytes } => format!("cleaned {}", format_bytes(*bytes)),
            Outcome::WouldClean { bytes } => format!("would clean {}", format_bytes(*bytes)),
            Outcome::Skipped { reason } => format!("skipped: {reason}"),
            Outcome::Failed { reason } => format!("failed: {reason}"),
        };
        println!("  {:<50} {status}", truncate(&o.label, 50));
    }
}

pub fn report(report: &CleanReport, history: &Path) {
    println!();
    let problems = report.failed + report.skipped;
    if report.dry_run {
        println!(
            "Dry run: would clean {}. Nothing was changed.",
            format_bytes(
                report
                    .outcomes
                    .iter()
                    .map(|o| match o.outcome {
                        Outcome::WouldClean { bytes } => bytes,
                        _ => 0,
                    })
                    .sum()
            )
        );
    } else {
        println!("Cleaned {}.", format_bytes(report.cleaned_bytes));
        if report.trashed_bytes > 0 {
            println!(
                "{} is in the Trash. Empty the Trash to free it.",
                format_bytes(report.trashed_bytes)
            );
        }
    }
    if problems > 0 {
        println!("{problems} items weren't cleaned; reasons are listed above.");
    }
    println!("Logged to {}", history.display());
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
