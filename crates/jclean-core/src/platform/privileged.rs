//! Administrator operations (spec §7.4), kept to a minimum: one
//! `do shell script … with administrator privileges` per clean, built only
//! from allowlisted commands and SafetyGuard-verified paths, with every
//! argument quoted for the shell and then escaped for AppleScript.

use std::path::{Path, PathBuf};

/// Tools that may ever run as administrator.
const ALLOWED_TOOLS: &[&str] = &["tmutil"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedOp {
    /// An allowlisted tool, e.g. `tmutil deletelocalsnapshots 2024-05-01-120000`.
    Command { program: PathBuf, args: Vec<String> },
    /// Removes a path the SafetyGuard has just verified. `rm -rf` removes
    /// symlinks as links and never follows them.
    Remove(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrivilegedError {
    #[error("{0} isn't allowed to run as administrator")]
    NotAllowed(String),
    #[error("the path isn't safe to pass to an administrator command")]
    UnsafePath,
}

/// POSIX single-quoting: safe for any bytes except NUL.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn clean_text(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// The shell line for one op, reporting `ok:<n>` or `fail:<n>`.
fn line(n: usize, op: &PrivilegedOp) -> Result<String, PrivilegedError> {
    let cmd = match op {
        PrivilegedOp::Command { program, args } => {
            let name = program
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let program_str = program.to_str().ok_or(PrivilegedError::UnsafePath)?;
            if !ALLOWED_TOOLS.contains(&name) || !program.is_absolute() {
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
            if !path.is_absolute() || !clean_text(p) || path == Path::new("/") {
                return Err(PrivilegedError::UnsafePath);
            }
            format!("/bin/rm -rf -- {}", shell_quote(p))
        }
    };
    Ok(format!(
        "{cmd} >/dev/null 2>&1 && echo ok:{n} || echo fail:{n}"
    ))
}

/// The AppleScript for `osascript -e`, running every op after one password
/// prompt.
pub fn script(ops: &[PrivilegedOp], prompt: &str) -> Result<String, PrivilegedError> {
    let lines: Vec<String> = ops
        .iter()
        .enumerate()
        .map(|(n, op)| line(n, op))
        .collect::<Result<_, _>>()?;
    Ok(format!(
        "do shell script {} with prompt {} with administrator privileges",
        applescript_string(&lines.join("; ")),
        applescript_string(prompt)
    ))
}

/// Which ops succeeded, from the script's output.
pub fn succeeded(output: &str, count: usize) -> Vec<bool> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_hostile_paths_for_shell_and_applescript() {
        let path = PathBuf::from("/Library/Logs/it's \"quoted\" $(rm -rf ~) \\ back");
        let s = script(&[PrivilegedOp::Remove(path)], "JClean needs your password.").unwrap();
        assert!(s.starts_with("do shell script \"/bin/rm -rf -- '"));
        // The single quote is closed, escaped and reopened; the rest is inert.
        assert!(
            s.contains(r#"it'\\''s \"quoted\" $(rm -rf ~) \\ back'"#),
            "{s}"
        );
        assert!(s.ends_with("with administrator privileges"));
    }

    #[test]
    fn only_allowlisted_tools_and_clean_absolute_paths() {
        let cmd = |p: &str| PrivilegedOp::Command {
            program: PathBuf::from(p),
            args: vec!["deletelocalsnapshots".into(), "2024-05-01-120000".into()],
        };
        assert!(script(&[cmd("/usr/bin/tmutil")], "x").is_ok());
        assert!(matches!(
            script(&[cmd("/bin/sh")], "x"),
            Err(PrivilegedError::NotAllowed(_))
        ));
        assert!(matches!(
            script(&[cmd("tmutil")], "x"),
            Err(PrivilegedError::NotAllowed(_))
        ));
        for bad in ["relative/path", "/", "/Library/Logs/new\nline"] {
            assert!(
                script(&[PrivilegedOp::Remove(PathBuf::from(bad))], "x").is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn reads_per_op_results() {
        assert_eq!(
            succeeded("ok:0\nfail:1\nok:2\n", 3),
            vec![true, false, true]
        );
        assert_eq!(succeeded("garbage", 2), vec![false, false]);
    }
}
