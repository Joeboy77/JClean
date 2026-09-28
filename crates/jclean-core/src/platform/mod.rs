//! OS-specific paths and behavior. Nothing outside this module (and the rule
//! files) should know where an OS keeps things.
//!
//! Functions dispatch on [`Env::os`] rather than `cfg`, so the macOS layout
//! can be tested on any machine against a fixture root.

pub mod fs;
mod linux;
mod macos;
pub mod privileged;
mod windows;

use std::path::PathBuf;

use crate::env::{Env, Os};

/// How much of the tree around a protected path is off limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The path itself and its ancestors. Its contents may be cleaned.
    Exact,
    /// The path, its ancestors, and everything inside it.
    Subtree,
    /// Carves a cleanup target out of a protected subtree: this folder and
    /// what's inside it may be cleaned, though an ancestor is `Subtree`.
    /// Its ancestors stay protected.
    CleanInside,
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
        Os::Windows => windows::known_dir(env, token),
        Os::Linux => linux::known_dir(env, token),
    }
}

/// Spec §7.2. The guard refuses these no matter what a rule says.
pub fn protected_paths(env: &Env) -> Vec<ProtectedPath> {
    match env.os() {
        Os::Macos => macos::protected_paths(env),
        Os::Windows => windows::protected_paths(env),
        Os::Linux => linux::protected_paths(env),
    }
}

/// Cloud-synced folders. Their contents are never walked (spec §4.2).
pub fn cloud_dirs(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::cloud_dirs(env),
        Os::Windows => windows::cloud_dirs(env),
        Os::Linux => linux::cloud_dirs(env),
    }
}

/// Folders skipped when discovering projects and walking for the disk map
/// (spec §4.5): system data, the Trash and cloud folders.
pub fn default_scan_exclusions(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::default_scan_exclusions(env),
        Os::Windows => windows::default_scan_exclusions(env),
        Os::Linux => linux::default_scan_exclusions(env),
    }
}

/// Where external tools are looked for (spec §7.4). GUI apps don't inherit
/// the shell `PATH`, so this is a fixed list.
pub fn tool_dirs(env: &Env) -> Vec<PathBuf> {
    match env.os() {
        Os::Macos => macos::tool_dirs(env),
        Os::Windows => windows::tool_dirs(env),
        Os::Linux => linux::tool_dirs(env),
    }
}

/// Colour bucket for a folder in the full-scan disk map. Falls back to the
/// parent's category, so everything under `~/Pictures` stays media.
pub fn categorize(
    env: &Env,
    path: &std::path::Path,
    parent: crate::rules::Category,
) -> crate::rules::Category {
    match env.os() {
        Os::Macos => macos::categorize(env, path, parent),
        Os::Windows => windows::categorize(env, path, parent),
        Os::Linux => linux::categorize(env, path, parent),
    }
}

/// Whether JClean has Full Disk Access (spec §11): tries to list a folder
/// macOS only shows to apps that have it. Lists names only; opens no file.
/// Windows has no equivalent, so it's always granted there.
pub fn has_full_disk_access(env: &Env) -> bool {
    match env.os() {
        Os::Macos => macos::has_full_disk_access(env),
        Os::Windows | Os::Linux => true,
    }
}

/// `~/Library/Application Support/app.jclean` on macOS, and
/// `%LOCALAPPDATA%\app.jclean` on Windows (spec §10).
pub fn app_data_dir(env: &Env) -> PathBuf {
    match env.os() {
        Os::Macos => macos::app_data_dir(env),
        Os::Windows => windows::app_data_dir(env),
        Os::Linux => linux::app_data_dir(env),
    }
}

/// The Windows build number (22000 and up is Windows 11), or `None` on
/// other systems.
pub fn windows_build() -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    let version = sysinfo::System::kernel_version()?;
    version.rsplit('.').next()?.trim().parse().ok()
}

/// The built-in rule for the Trash (macOS) or Recycle Bin (Windows), which
/// "Empty Trash" cleans on its own.
pub fn trash_rule_id(os: Os) -> &'static str {
    match os {
        Os::Macos => "macos.system.trash",
        Os::Windows => "windows.system.recycle-bin",
        Os::Linux => "linux.system.trash",
    }
}

/// Folders that are developer storage wherever they appear, on any OS.
const DEV_NAMES: &[&str] = &[
    "node_modules",
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".bun",
    ".nvm",
    ".volta",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    ".android",
    ".pub-cache",
    ".docker",
    ".cache",
    ".nuget",
    "go",
    "Developer",
    "target",
    ".venv",
    "venv",
    "DerivedData",
    "CoreSimulator",
];
