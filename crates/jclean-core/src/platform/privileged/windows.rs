//! Windows: one UAC prompt per clean. A hidden PowerShell asks Windows to
//! start a second, elevated PowerShell with the script below.
//!
//! - Paths are single-quoted PowerShell literals, so nothing in them expands
//!   or runs. The Unicode quotes PowerShell also accepts are escaped too.
//! - Folders are removed with .NET's `Directory.Delete`, which removes a
//!   junction or symlink as a link and never recurses through one.
//! - Results come back in the elevated process's exit code (one bit per op),
//!   so nothing is written to a folder another program could tamper with.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{AdminError, PrivilegedError, PrivilegedOp};
use crate::tools::CommandRunner;

/// Set in every exit code the script produces, so it can't be confused with
/// a crash or a refused prompt. The low bits are the ops that failed.
const REPORT: u32 = 0x1000_0000;
/// One bit per op.
const MAX_OPS: usize = 20;
/// The launcher's exit code when UAC is declined (ERROR_CANCELLED).
const CANCELLED: i32 = 1223;
/// Windows' command-line limit, with room for the launcher itself.
const MAX_COMMAND_LINE: usize = 30_000;

/// The exact argument lists allowed per tool.
fn allowed(name: &str, args: &[String]) -> bool {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match name.to_ascii_lowercase().as_str() {
        // Component store cleanup (spec §8.3).
        "dism.exe" => args == ["/Online", "/Cleanup-Image", "/StartComponentCleanup"],
        // Delivery Optimization cache (spec §8.3).
        "powershell.exe" => {
            args == [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Delete-DeliveryOptimizationCache -Force",
            ]
        }
        _ => false,
    }
}

/// A PowerShell single-quoted literal. Inside one, only quote characters
/// are special: `'` and the curly quotes PowerShell treats the same way.
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn clean_text(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(char::is_control)
}

/// A verified absolute path that isn't a drive root.
fn safe_path(path: &Path) -> Result<String, PrivilegedError> {
    let text = path.to_str().ok_or(PrivilegedError::UnsafePath)?;
    let is_absolute = path.is_absolute() || text.get(1..3) == Some(":\\");
    let depth = text
        .trim_end_matches('\\')
        .split('\\')
        .filter(|s| !s.is_empty())
        .count();
    if !is_absolute || !clean_text(text) || depth < 2 {
        return Err(PrivilegedError::UnsafePath);
    }
    Ok(ps_quote(text))
}

/// Removes one entry. Reparse points (symlinks, junctions, mount points) go
/// as links; read-only files are made writable first.
const PREAMBLE: &str = "$ErrorActionPreference = 'Stop'
$failed = 0
function Remove-JCleanItem([string]$p) {
  $a = [IO.File]::GetAttributes($p)
  if ($a -band [IO.FileAttributes]::ReparsePoint) {
    if ($a -band [IO.FileAttributes]::Directory) { [IO.Directory]::Delete($p, $false) } else { [IO.File]::Delete($p) }
  } elseif ($a -band [IO.FileAttributes]::Directory) {
    [IO.Directory]::Delete($p, $true)
  } else {
    [IO.File]::SetAttributes($p, [IO.FileAttributes]::Normal)
    [IO.File]::Delete($p)
  }
}
function Clear-JCleanFolder([string]$p) {
  $bad = $false
  foreach ($c in [IO.Directory]::GetFileSystemEntries($p)) {
    try { Remove-JCleanItem $c } catch { $bad = $true }
  }
  if ($bad) { throw 'Part of it could not be removed' }
}
";

/// The elevated script. It exits with [`REPORT`] plus a bit per failed op.
pub(super) fn script(ops: &[PrivilegedOp]) -> Result<String, PrivilegedError> {
    if ops.len() > MAX_OPS {
        return Err(PrivilegedError::TooMany);
    }
    let mut out = String::from(PREAMBLE);
    for (n, op) in ops.iter().enumerate() {
        let body = match op {
            PrivilegedOp::Remove(path) => format!("Remove-JCleanItem {}", safe_path(path)?),
            PrivilegedOp::ClearContents(path) => {
                format!("Clear-JCleanFolder {}", safe_path(path)?)
            }
            PrivilegedOp::Command { program, args } => {
                // After the last separator, whichever the path uses.
                let name = program
                    .to_str()
                    .and_then(|p| p.rsplit(['\\', '/']).next())
                    .unwrap_or_default();
                if !allowed(name, args) {
                    return Err(PrivilegedError::NotAllowed(name.to_string()));
                }
                let quoted: Vec<String> = args.iter().map(|a| ps_quote(a)).collect();
                format!(
                    "& {} {} | Out-Null; if ($LASTEXITCODE -ne 0) {{ throw 'exit' }}",
                    safe_path(program)?,
                    quoted.join(" ")
                )
            }
        };
        out.push_str(&format!(
            "try {{ {body} }} catch {{ $failed = $failed -bor {} }}\n",
            1u32 << n
        ));
    }
    out.push_str(&format!("exit ({REPORT} -bor $failed)\n"));
    Ok(out)
}

/// `-EncodedCommand` takes base64 of UTF-16LE, which sidesteps every layer
/// of command-line quoting.
fn encode(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The non-elevated launcher: asks for approval and waits, passing the
/// elevated script's exit code through, or [`CANCELLED`].
pub(super) fn launcher(powershell: &Path, ops: &[PrivilegedOp]) -> Result<String, PrivilegedError> {
    let encoded = encode(&script(ops)?);
    let command = format!(
        "try {{ $p = Start-Process -FilePath {} -ArgumentList '-NoProfile','-NonInteractive','-WindowStyle','Hidden','-EncodedCommand','{encoded}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru }} catch {{ exit {CANCELLED} }}; exit $p.ExitCode",
        safe_path(powershell)?
    );
    if command.len() > MAX_COMMAND_LINE {
        return Err(PrivilegedError::TooMany);
    }
    Ok(command)
}

/// Which ops succeeded, from the elevated script's exit code.
pub(super) fn succeeded(code: Option<i32>, count: usize) -> Result<Vec<bool>, AdminError> {
    match code {
        Some(CANCELLED) => Err(AdminError::Cancelled),
        Some(c) if u32::from_ne_bytes(c.to_ne_bytes()) & REPORT != 0 => {
            let failed = u32::from_ne_bytes(c.to_ne_bytes()) & !REPORT;
            Ok((0..count).map(|n| failed & (1 << n) == 0).collect())
        }
        _ => Err(AdminError::Failed(
            "the administrator step stopped before it finished".to_string(),
        )),
    }
}

fn windows_powershell() -> PathBuf {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

pub(super) fn run(
    ops: &[PrivilegedOp],
    runner: &dyn CommandRunner,
    timeout: Duration,
) -> Result<Vec<bool>, AdminError> {
    let powershell = windows_powershell();
    let command = launcher(&powershell, ops)?;
    let out = runner
        .run(
            &powershell,
            &[
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                command,
            ],
            timeout,
        )
        .map_err(|e| AdminError::Failed(e.to_string()))?;
    succeeded(out.status, ops.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(b64: &str) -> String {
        let value = |c: u8| match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            _ => 63,
        };
        let clean: Vec<u8> = b64.bytes().filter(|&c| c != b'=').collect();
        let mut bytes = Vec::new();
        for chunk in clean.chunks(4) {
            let mut n = 0u32;
            for (i, &c) in chunk.iter().enumerate() {
                n |= u32::from(value(c)) << (18 - 6 * i);
            }
            for i in 0..chunk.len().saturating_sub(1) {
                bytes.push((n >> (16 - 8 * i)) as u8);
            }
        }
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
            .collect();
        String::from_utf16_lossy(&units)
    }

    #[test]
    fn quotes_hostile_paths_as_inert_literals() {
        let path = PathBuf::from("C:\\Windows.old\\it's ‘curly’ $(Remove-Item C:\\) `n $env:TEMP");
        let s = script(&[PrivilegedOp::Remove(path)]).unwrap();
        assert!(
            s.contains("Remove-JCleanItem 'C:\\Windows.old\\it''s ‘‘curly’’ $(Remove-Item C:\\) `n $env:TEMP'"),
            "{s}"
        );
    }

    #[test]
    fn refuses_roots_relative_and_control_characters() {
        for bad in ["C:\\", "C:", "relative\\path", "C:\\Temp\\new\nline", ""] {
            assert!(
                script(&[PrivilegedOp::Remove(PathBuf::from(bad))]).is_err(),
                "{bad:?}"
            );
        }
        assert!(
            script(&[PrivilegedOp::ClearContents(PathBuf::from(
                "C:\\Windows\\SoftwareDistribution\\Download"
            ))])
            .is_ok()
        );
    }

    #[test]
    fn only_exact_allowlisted_commands() {
        let cmd = |p: &str, args: &[&str]| PrivilegedOp::Command {
            program: PathBuf::from(p),
            args: args.iter().map(ToString::to_string).collect(),
        };
        let dism = ["/Online", "/Cleanup-Image", "/StartComponentCleanup"];
        assert!(script(&[cmd("C:\\Windows\\System32\\Dism.exe", &dism)]).is_ok());
        assert!(matches!(
            script(&[cmd(
                "C:\\Windows\\System32\\Dism.exe",
                &["/Online", "/Cleanup-Image", "/ResetBase"]
            )]),
            Err(PrivilegedError::NotAllowed(_))
        ));
        assert!(matches!(
            script(&[cmd("C:\\Windows\\System32\\cmd.exe", &["/c", "rd"])]),
            Err(PrivilegedError::NotAllowed(_))
        ));
        assert!(matches!(
            script(&[cmd(
                "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
                &["-Command", "Remove-Item C:\\"]
            )]),
            Err(PrivilegedError::NotAllowed(_))
        ));
        assert!(
            script(&[cmd("Dism.exe", &dism)]).is_err(),
            "relative program"
        );
    }

    #[test]
    fn launcher_carries_the_script_encoded() {
        let ops = [PrivilegedOp::Remove(PathBuf::from("C:\\Windows.old"))];
        let command = launcher(
            Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
            &ops,
        )
        .unwrap();
        assert!(
            !command.contains('"'),
            "no double quotes for the outer command line"
        );
        let encoded = command
            .split("'-EncodedCommand','")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .unwrap();
        assert_eq!(decode(encoded), script(&ops).unwrap());
        assert!(command.contains("-Verb RunAs"));
    }

    #[test]
    fn caps_the_number_of_ops() {
        let ops: Vec<PrivilegedOp> = (0..=MAX_OPS)
            .map(|n| PrivilegedOp::Remove(PathBuf::from(format!("C:\\Windows.old\\{n}"))))
            .collect();
        assert_eq!(script(&ops), Err(PrivilegedError::TooMany));
    }

    #[test]
    fn reads_results_from_the_exit_code() {
        let code = |failed: u32| Some(i32::from_ne_bytes((REPORT | failed).to_ne_bytes()));
        assert_eq!(succeeded(code(0), 3), Ok(vec![true, true, true]));
        assert_eq!(succeeded(code(0b010), 3), Ok(vec![true, false, true]));
        assert_eq!(succeeded(Some(CANCELLED), 2), Err(AdminError::Cancelled));
        assert!(matches!(succeeded(Some(1), 2), Err(AdminError::Failed(_))));
        assert!(matches!(succeeded(None, 2), Err(AdminError::Failed(_))));
    }
}
