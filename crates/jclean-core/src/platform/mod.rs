//! OS-specific paths and behavior. Nothing outside this module (and the rule
//! files) should know where an OS keeps things.
//!
//! Functions dispatch on [`Env::os`] rather than `cfg`, so the macOS layout
//! can be tested on any machine against a fixture root.

pub mod fs;
mod macos;

use std::path::PathBuf;

use crate::env::{Env, Os};

/// How much of the tree around a protected path is off limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The path itself and its ancestors. Its contents may be cleaned.
    Exact,
    /// The path, its ancestors, and everything inside it.
    Subtree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedPath {
    pub path: PathBuf,
    pub scope: Scope,
}

/// Resolves a rule path token such as `caches` or `appSupport`.
/// Returns `None` when the token has no meaning on this OS.
pub fn known_dir(env: &Env, token: &str) -> Option<PathBuf> {
    match env.os() {
        Os::Macos => macos::known_dir(env, token),
        Os::Windows | Os::Linux => None,
    }
}

/// Spec §7.2. The guard refuses these no matter what a rule says.
pub fn protected_paths(env: &Env) -> Vec<ProtectedPath> {
    match env.os() {
        Os::Macos => macos::protected_paths(env),
        Os::Windows | Os::Linux => vec![ProtectedPath {
            path: env.root().to_path_buf(),
            scope: Scope::Exact,
        }],
    }
}

/// Cloud-synced folders. Their contents are never walked (spec §4.2).
pub fn cloud_dirs(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::cloud_dirs(env),
        Os::Windows | Os::Linux => Vec::new(),
    }
}

/// Folders skipped when discovering projects and walking for the disk map
/// (spec §4.5): system data, the Trash and cloud folders.
pub fn default_scan_exclusions(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::default_scan_exclusions(env),
        Os::Windows | Os::Linux => Vec::new(),
    }
}

/// Where external tools are looked for (spec §7.4). GUI apps don't inherit
/// the shell `PATH`, so this is a fixed list.
pub fn tool_dirs(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::tool_dirs(env),
        Os::Windows | Os::Linux => Vec::new(),
    }
}

/// `~/Library/Application Support/app.jclean` on macOS (spec §10).
pub fn app_data_dir(env: &Env) -> PathBuf {
    match env.os() {
        Os::Macos => macos::app_data_dir(env),
        Os::Windows | Os::Linux => env.home().join(".jclean"),
    }
}
