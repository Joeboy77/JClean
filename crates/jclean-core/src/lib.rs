//! JClean's core engine: scanning, sizing, rules, planning, safety and cleaning.
//!
//! This crate has no Tauri dependency so it can be tested in isolation and
//! driven by `jclean-cli`. See `docs/SPEC.md` §3 for the module layout.
//!
//! Deletion happens in exactly one place, [`cleaner::execute`], and every path
//! passes the [`safety::SafetyGuard`] first.

pub mod cancel;
pub mod cleaner;
pub mod disktree;
pub mod env;
pub mod history;
pub mod planner;
pub mod platform;
pub mod probes;
pub mod projects;
pub mod rules;
pub mod safety;
pub mod scanner;
pub mod sizing;
pub mod time;
pub mod tools;
pub mod units;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

/// Version of the core engine, taken from the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
