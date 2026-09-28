//! Linux: one polkit prompt (`pkexec`) per clean, running the same shell
//! lines as macOS through `/bin/sh -c`.

use std::path::Path;
use std::time::Duration;

use super::posix::{line, succeeded};
use super::{AdminError, PrivilegedError, PrivilegedOp};
use crate::tools::CommandRunner;

const PKEXEC: &str = "/usr/bin/pkexec";
/// pkexec's exit code when the password dialog is dismissed.
const DISMISSED: i32 = 126;
/// pkexec's exit code when authorization fails.
const NOT_AUTHORIZED: i32 = 127;

/// The exact argument lists allowed per tool (spec §7.4).
fn allowed(name: &str, args: &[String]) -> bool {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match name {
        "apt-get" => args == ["clean"],
        "dnf" => args == ["clean", "packages"],
        "journalctl" => args == ["--vacuum-time=2weeks"],
        // One disabled revision of one snap: `snap remove <name> --revision=<n>`.
        "snap" => match args.as_slice() {
            ["remove", name, revision] => {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                    && !name.starts_with('-')
                    && revision
                        .strip_prefix("--revision=")
                        .is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()))
            }
            _ => false,
        },
        _ => false,
    }
}

/// The script `pkexec /bin/sh -c` runs.
pub(super) fn script(ops: &[PrivilegedOp]) -> Result<String, PrivilegedError> {
    let lines: Vec<String> = ops
        .iter()
        .enumerate()
        .map(|(n, op)| line(n, op, allowed))
        .collect::<Result<_, _>>()?;
    Ok(lines.join("; "))
}

pub(super) fn run(
    ops: &[PrivilegedOp],
    runner: &dyn CommandRunner,
    timeout: Duration,
) -> Result<Vec<bool>, AdminError> {
    let script = script(ops)?;
    let out = runner
        .run(
            Path::new(PKEXEC),
            &["/bin/sh".to_string(), "-c".to_string(), script],
            timeout,
        )
        .map_err(|e| {
            AdminError::Failed(format!(
                "{e}. Administrator cleanups need polkit (pkexec) to be installed."
            ))
        })?;
    match out.status {
        Some(0) => Ok(succeeded(&out.stdout, ops.len())),
        Some(DISMISSED) => Err(AdminError::Cancelled),
        Some(NOT_AUTHORIZED) => Err(AdminError::Failed(
            "your account isn't allowed to approve administrator tasks".to_string(),
        )),
        _ => Err(AdminError::Failed(
            out.stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("it didn't finish")
                .trim()
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn cmd(program: &str, args: &[&str]) -> PrivilegedOp {
        PrivilegedOp::Command {
            program: PathBuf::from(program),
            args: args.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn only_exact_allowlisted_commands() {
        assert!(script(&[cmd("/usr/bin/apt-get", &["clean"])]).is_ok());
        assert!(script(&[cmd("/usr/bin/dnf", &["clean", "packages"])]).is_ok());
        assert!(script(&[cmd("/usr/bin/journalctl", &["--vacuum-time=2weeks"])]).is_ok());
        assert!(
            script(&[cmd(
                "/usr/bin/snap",
                &["remove", "firefox", "--revision=4793"]
            )])
            .is_ok()
        );
        for (program, args) in [
            ("/usr/bin/apt-get", &["autoremove", "-y"][..]),
            ("/usr/bin/dnf", &["remove", "kernel"][..]),
            ("/usr/bin/journalctl", &["--vacuum-time=1s", "--rotate"][..]),
            ("/usr/bin/snap", &["remove", "firefox"][..]),
            ("/usr/bin/snap", &["remove", "--purge", "--revision=1"][..]),
            ("/usr/bin/snap", &["remove", "fire fox", "--revision=1"][..]),
            (
                "/usr/bin/snap",
                &["remove", "firefox", "--revision=1;x"][..],
            ),
            ("/bin/sh", &["-c", "rm -rf /"][..]),
            ("apt-get", &["clean"][..]),
        ] {
            assert!(
                script(&[cmd(program, args)]).is_err(),
                "{program} {args:?} must be refused"
            );
        }
    }

    /// Every administrator command in the Linux rules must be on the
    /// allowlist, with a probe key filled in where the rule takes one.
    #[test]
    fn every_linux_admin_command_is_allowlisted() {
        use crate::env::Os;
        use crate::rules::{Method, RuleSet};

        let rules = RuleSet::builtin(Os::Linux).unwrap();
        let mut checked = 0;
        for rule in rules.rules() {
            let c = &rule.cleanup;
            if !(c.requires_admin && c.method == Method::Command) {
                continue;
            }
            let command = c.command.as_ref().unwrap();
            let args: Vec<String> = command
                .args
                .iter()
                .map(|a| a.replace("{item.0}", "core20").replace("{item.1}", "1828"))
                .collect();
            assert!(
                allowed(&command.tool, &args),
                "{} isn't allowlisted",
                rule.id
            );
            checked += 1;
        }
        assert_eq!(checked, 4);
    }

    #[test]
    fn runs_every_op_in_one_shell_with_per_op_results() {
        let s = script(&[
            cmd("/usr/bin/apt-get", &["clean"]),
            cmd("/usr/bin/journalctl", &["--vacuum-time=2weeks"]),
        ])
        .unwrap();
        assert!(s.starts_with(
            "'/usr/bin/apt-get' 'clean' >/dev/null 2>&1 && echo ok:0 || echo fail:0; "
        ));
        assert!(s.ends_with("echo ok:1 || echo fail:1"));
    }
}
