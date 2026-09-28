//! Administrator operations (spec §7.4), kept to a minimum: one approval per
//! clean (a password prompt on macOS, a UAC prompt on Windows), built only
//! from allowlisted commands and SafetyGuard-verified paths.
//!
//! Only [`crate::cleaner`] calls this.

mod macos;
mod windows;

use std::path::PathBuf;
use std::time::Duration;

use crate::tools::CommandRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedOp {
    /// An allowlisted tool, e.g. `tmutil deletelocalsnapshots 2024-05-01-120000`.
    Command { program: PathBuf, args: Vec<String> },
    /// Removes a path the SafetyGuard has just verified. A symlink or
    /// junction is removed as a link and never followed.
    Remove(PathBuf),
    /// Removes everything inside a verified folder, keeping the folder.
    ClearContents(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrivilegedError {
    #[error("{0} isn't allowed to run as administrator")]
    NotAllowed(String),
    #[error("the path isn't safe to pass to an administrator command")]
    UnsafePath,
    #[error("too many administrator items at once; clean them in smaller groups")]
    TooMany,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdminError {
    #[error(transparent)]
    Refused(#[from] PrivilegedError),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

/// Runs `ops` after one approval and reports which succeeded, in order.
pub fn run(
    ops: &[PrivilegedOp],
    prompt: &str,
    runner: &dyn CommandRunner,
    timeout: Duration,
) -> Result<Vec<bool>, AdminError> {
    if cfg!(windows) {
        windows::run(ops, runner, timeout)
    } else {
        macos::run(ops, prompt, runner, timeout)
    }
}

/// Whether a folder's contents are cleared as one op. On macOS the cleaner
/// lists the folder and removes each entry, so each can succeed on its own.
pub fn clears_contents_in_one_op() -> bool {
    cfg!(windows)
}

/// What the approval prompt calls the OS's own refusal, for error messages.
pub fn os_name() -> &'static str {
    if cfg!(windows) { "Windows" } else { "macOS" }
}
