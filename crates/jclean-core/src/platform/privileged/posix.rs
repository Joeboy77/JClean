//! The shell side of administrator operations on macOS and Linux: one line
//! per op, every argument POSIX single-quoted, each reporting `ok:<n>` or
//! `fail:<n>` so a partial failure is attributed to the right item.

use std::path::Path;

use super::{PrivilegedError, PrivilegedOp};

/// POSIX single-quoting: safe for any bytes except NUL.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn clean_text(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// The shell line for one op, reporting `ok:<n>` or `fail:<n>`.
pub(super) fn line(
    n: usize,
    op: &PrivilegedOp,
    allowed: fn(&str, &[String]) -> bool,
) -> Result<String, PrivilegedError> {
    let cmd = match op {
        PrivilegedOp::Command { program, args } => {
            let name = program
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let program_str = program.to_str().ok_or(PrivilegedError::UnsafePath)?;
            if !allowed(name, args) || !program_str.starts_with('/') {
                return Err(PrivilegedError::NotAllowed(name.to_string()));
            }
            if !clean_text(program_str) || args.iter().any(|a| !clean_text(a)) {
                return Err(PrivilegedError::UnsafePath);
            }
            std::iter::once(shell_quote(program_str))
                .chain(args.iter().map(|a| shell_quote(a)))
                .collect::<Vec<_>>()
                .join(" ")
        }
        PrivilegedOp::Remove(path) => {
            let p = path.to_str().ok_or(PrivilegedError::UnsafePath)?;
            if !p.starts_with('/') || !clean_text(p) || path == Path::new("/") {
                return Err(PrivilegedError::UnsafePath);
            }
            format!("/bin/rm -rf -- {}", shell_quote(p))
        }
        PrivilegedOp::ClearContents(path) => {
            let p = path.to_str().ok_or(PrivilegedError::UnsafePath)?;
            if !p.starts_with('/') || !clean_text(p) || path == Path::new("/") {
                return Err(PrivilegedError::UnsafePath);
            }
            // `find` doesn't follow symlinks, and `rm -rf` removes them as links.
            format!(
                "/usr/bin/find {} -mindepth 1 -maxdepth 1 -exec /bin/rm -rf -- {{}} +",
                shell_quote(p)
            )
        }
    };
    Ok(format!(
        "{cmd} >/dev/null 2>&1 && echo ok:{n} || echo fail:{n}"
    ))
}

/// Which ops succeeded, from the script's output.
pub(super) fn succeeded(output: &str, count: usize) -> Vec<bool> {
    let mut ok = vec![false; count];
    for line in output.lines() {
        if let Some(n) = line
            .trim()
            .strip_prefix("ok:")
            .and_then(|n| n.parse::<usize>().ok())
            && let Some(slot) = ok.get_mut(n)
        {
            *slot = true;
        }
    }
    ok
}
