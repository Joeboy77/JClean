//! macOS: one `do shell script … with administrator privileges` per clean,
//! with every argument quoted for the shell and then escaped for AppleScript.

use std::path::Path;
use std::time::Duration;

use super::posix::{line, succeeded};
use super::{AdminError, PrivilegedError, PrivilegedOp};
use crate::tools::CommandRunner;

/// Tools that may ever run as administrator, with any arguments.
fn allowed(name: &str, _args: &[String]) -> bool {
    name == "tmutil"
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The AppleScript for `osascript -e`, running every op after one password
/// prompt.
pub(super) fn script(ops: &[PrivilegedOp], prompt: &str) -> Result<String, PrivilegedError> {
    let lines: Vec<String> = ops
        .iter()
        .enumerate()
        .map(|(n, op)| line(n, op, allowed))
        .collect::<Result<_, _>>()?;
    Ok(format!(
        "do shell script {} with prompt {} with administrator privileges",
        applescript_string(&lines.join("; ")),
        applescript_string(prompt)
    ))
}

const OSASCRIPT: &str = "/usr/bin/osascript";

/// Runs `ops` after one password prompt.
pub(super) fn run(
    ops: &[PrivilegedOp],
    prompt: &str,
    runner: &dyn CommandRunner,
    timeout: Duration,
) -> Result<Vec<bool>, AdminError> {
    let script = script(ops, prompt)?;
    let out = runner
        .run(Path::new(OSASCRIPT), &["-e".to_string(), script], timeout)
        .map_err(|e| AdminError::Failed(e.to_string()))?;
    if out.success() {
        return Ok(succeeded(&out.stdout, ops.len()));
    }
    // AppleScript error -128: the password prompt was cancelled.
    if out.stderr.contains("-128") {
        return Err(AdminError::Cancelled);
    }
    Err(AdminError::Failed(
        out.stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it didn't finish")
            .trim()
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
