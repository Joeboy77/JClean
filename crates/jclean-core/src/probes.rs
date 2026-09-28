//! Probes: asking tools about storage they manage (spec §3, §8). Probes only
//! read; any cleanup goes through the planner and cleaner like everything else.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::cancel::CancelToken;
use crate::env::Env;
use crate::sizing::{InodeSet, MeasureOptions, measure};
use crate::tools::{CommandError, CommandOutput, CommandRunner};

pub const KNOWN_PROBES: &[&str] = &[
    "docker-build-cache",
    "docker-images",
    "docker-volumes",
    "simctl-unavailable",
    "tmutil-snapshots",
    "delivery-optimization",
    "component-store",
    "journal-disk-usage",
    "snap-disabled",
    "flatpak-leftovers",
];

const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeItem {
    /// Substituted for `{item}` in the rule's cleanup command.
    pub key: String,
    /// Shown after the rule label, e.g. a snapshot's date.
    pub name: Option<String>,
    /// `None` when the tool can't say how big it is.
    pub bytes: Option<u64>,
    /// Folders the item lives in, if known. Used to avoid double counting.
    pub paths: Vec<PathBuf>,
    /// Set when the item is one folder (`paths[0]`) that the rule's delete or
    /// trash method cleans like any other path: the folder it must stay
    /// inside for the SafetyGuard.
    pub root: Option<PathBuf>,
    /// The folder's own modification time, for the guard's changed-since-scan
    /// check. Only meaningful with `root`.
    pub own_mtime: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("{0} isn't installed")]
    ToolMissing(String),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("{tool} reported an error: {message}")]
    Failed { tool: String, message: String },
    #[error("couldn't understand {tool}'s output: {message}")]
    Parse { tool: String, message: String },
    #[error("unknown probe {0}")]
    Unknown(String),
}

/// Runs probes for one scan, caching each tool invocation so the three
/// Docker probes share one `docker system df`.
pub struct Prober<'a> {
    env: &'a Env,
    runner: &'a dyn CommandRunner,
    cache: Mutex<HashMap<String, Result<CommandOutput, String>>>,
}

impl<'a> Prober<'a> {
    pub fn new(env: &'a Env, runner: &'a dyn CommandRunner) -> Self {
        Self {
            env,
            runner,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn run(
        &self,
        probe: &str,
        inodes: &InodeSet,
        cancel: &CancelToken,
    ) -> Result<Vec<ProbeItem>, ProbeError> {
        match probe {
            "docker-build-cache" => self.docker("Build Cache", "build-cache"),
            "docker-images" => self.docker("Images", "images"),
            "docker-volumes" => self.docker("Local Volumes", "volumes"),
            "simctl-unavailable" => self.simctl_unavailable(inodes, cancel),
            "tmutil-snapshots" => self.tmutil_snapshots(),
            "delivery-optimization" => self.delivery_optimization(),
            "component-store" => self.component_store(),
            "journal-disk-usage" => self.journal_disk_usage(),
            "snap-disabled" => self.snap_disabled(inodes, cancel),
            "flatpak-leftovers" => self.flatpak_leftovers(inodes, cancel),
            other => Err(ProbeError::Unknown(other.to_string())),
        }
    }

    fn call(&self, tool: &str, args: &[&str]) -> Result<CommandOutput, ProbeError> {
        let key = format!("{tool} {}", args.join(" "));
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.get(&key)
        {
            return hit.clone().map_err(|message| ProbeError::Failed {
                tool: tool.to_string(),
                message,
            });
        }
        let program = self
            .runner
            .find_tool(self.env, tool)
            .ok_or_else(|| ProbeError::ToolMissing(tool.to_string()))?;
        let args: Vec<String> = args.iter().map(ToString::to_string).collect();
        let out = self.runner.run(&program, &args, PROBE_TIMEOUT)?;
        let result = if out.success() {
            Ok(out)
        } else {
            Err(first_line(&out.stderr)
                .unwrap_or("exited with an error")
                .to_string())
        };
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, result.clone());
        }
        result.map_err(|message| ProbeError::Failed {
            tool: tool.to_string(),
            message,
        })
    }

    fn docker(&self, kind: &str, key: &str) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call("docker", &["system", "df", "--format", "{{json .}}"])?;
        for line in out.stdout.lines().filter(|l| !l.trim().is_empty()) {
            let row: serde_json::Value =
                serde_json::from_str(line).map_err(|e| ProbeError::Parse {
                    tool: "docker".to_string(),
                    message: e.to_string(),
                })?;
            if row.get("Type").and_then(|t| t.as_str()) != Some(kind) {
                continue;
            }
            let reclaimable = row
                .get("Reclaimable")
                .and_then(|v| v.as_str())
                .unwrap_or("0B");
            let bytes = parse_docker_size(reclaimable).ok_or_else(|| ProbeError::Parse {
                tool: "docker".to_string(),
                message: format!("unexpected size {reclaimable:?}"),
            })?;
            if bytes == 0 {
                return Ok(Vec::new());
            }
            return Ok(vec![ProbeItem {
                key: key.to_string(),
                name: None,
                bytes: Some(bytes),
                paths: Vec::new(),
                root: None,
                own_mtime: 0,
            }]);
        }
        Ok(Vec::new())
    }

    fn simctl_unavailable(
        &self,
        inodes: &InodeSet,
        cancel: &CancelToken,
    ) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call(
            "xcrun",
            &["simctl", "list", "devices", "unavailable", "--json"],
        )?;
        let parse_err = |message: String| ProbeError::Parse {
            tool: "xcrun simctl".to_string(),
            message,
        };
        let json: serde_json::Value =
            serde_json::from_str(&out.stdout).map_err(|e| parse_err(e.to_string()))?;
        let devices = json
            .get("devices")
            .and_then(|d| d.as_object())
            .ok_or_else(|| parse_err("missing devices".to_string()))?;

        let base = self
            .env
            .home()
            .join("Library/Developer/CoreSimulator/Devices");
        let mut paths = Vec::new();
        let mut bytes = 0;
        for device in devices.values().filter_map(|v| v.as_array()).flatten() {
            let Some(udid) = device.get("udid").and_then(|u| u.as_str()) else {
                continue;
            };
            // UDIDs are hex and dashes; anything else is ignored rather than joined into a path.
            if udid.is_empty() || !udid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                continue;
            }
            let path = base.join(udid);
            if let Ok(m) = measure(&path, &MeasureOptions::default(), inodes, cancel) {
                bytes += m.allocated;
                paths.push(path);
            }
        }
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![ProbeItem {
            key: "unavailable".to_string(),
            name: Some(format!("{} simulators", paths.len())),
            bytes: Some(bytes),
            paths,
            root: None,
            own_mtime: 0,
        }])
    }

    fn tmutil_snapshots(&self) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call("tmutil", &["listlocalsnapshots", "/"])?;
        Ok(parse_snapshot_dates(&out.stdout)
            .into_iter()
            .map(|date| ProbeItem {
                name: Some(date.clone()),
                key: date,
                bytes: None,
                paths: Vec::new(),
                root: None,
                own_mtime: 0,
            })
            .collect())
    }
}

impl Prober<'_> {
    /// Windows' peer-to-peer update cache. Its folder belongs to a system
    /// account, so Windows is asked for the size instead.
    fn delivery_optimization(&self) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-DeliveryOptimizationPerfSnap).CacheSizeBytes",
            ],
        )?;
        let text = first_line(&out.stdout).unwrap_or("0");
        let bytes: u64 = text.parse().map_err(|_| ProbeError::Parse {
            tool: "Get-DeliveryOptimizationPerfSnap".to_string(),
            message: format!("unexpected size {text:?}"),
        })?;
        Ok((bytes > 0)
            .then(|| ProbeItem {
                key: "cache".to_string(),
                name: None,
                bytes: Some(bytes),
                paths: Vec::new(),
                root: None,
                own_mtime: 0,
            })
            .into_iter()
            .collect())
    }

    /// Superseded Windows components. Measuring them needs administrator
    /// rights, so the item is offered with its size unknown.
    fn component_store(&self) -> Result<Vec<ProbeItem>, ProbeError> {
        self.runner
            .find_tool(self.env, "Dism")
            .ok_or_else(|| ProbeError::ToolMissing("Dism".to_string()))?;
        Ok(vec![ProbeItem {
            key: "component-store".to_string(),
            name: None,
            bytes: None,
            paths: Vec::new(),
            root: None,
            own_mtime: 0,
        }])
    }
}

impl Prober<'_> {
    /// systemd's journal. Its files belong to the system, so journalctl is
    /// asked for their size.
    fn journal_disk_usage(&self) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call("journalctl", &["--disk-usage"])?;
        let bytes = parse_journal_usage(&out.stdout).ok_or_else(|| ProbeError::Parse {
            tool: "journalctl".to_string(),
            message: format!("unexpected output {:?}", first_line(&out.stdout)),
        })?;
        Ok((bytes > 0)
            .then(|| ProbeItem {
                key: "journal".to_string(),
                name: None,
                bytes: Some(bytes),
                paths: Vec::new(),
                root: None,
                own_mtime: 0,
            })
            .into_iter()
            .collect())
    }

    /// Snap keeps old revisions of every snap, disabled, after updating.
    fn snap_disabled(
        &self,
        inodes: &InodeSet,
        cancel: &CancelToken,
    ) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call("snap", &["list", "--all"])?;
        let snaps = self
            .env
            .system_path(std::path::Path::new("/var/lib/snapd/snaps"));
        Ok(parse_disabled_snaps(&out.stdout)
            .into_iter()
            .map(|(name, revision)| {
                let file = snaps.join(format!("{name}_{revision}.snap"));
                let bytes = measure(&file, &MeasureOptions::default(), inodes, cancel)
                    .ok()
                    .map(|m| m.allocated);
                ProbeItem {
                    key: format!("{name} {revision}"),
                    name: Some(format!("{name} (revision {revision})")),
                    bytes,
                    paths: vec![file],
                    root: None,
                    own_mtime: 0,
                }
            })
            .collect())
    }

    /// Data Flatpak apps left in ~/.var/app after they were uninstalled.
    fn flatpak_leftovers(
        &self,
        inodes: &InodeSet,
        cancel: &CancelToken,
    ) -> Result<Vec<ProbeItem>, ProbeError> {
        let out = self.call("flatpak", &["list", "--app", "--columns=application"])?;
        let installed: std::collections::HashSet<&str> =
            out.stdout.lines().map(str::trim).collect();
        let root = self.env.home().join(".var/app");
        let Ok(entries) = std::fs::read_dir(&root) else {
            return Ok(Vec::new());
        };
        let mut items = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let id = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            // App IDs are reverse-DNS names; anything else isn't Flatpak's.
            let looks_like_id = id.contains('.')
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
            if !is_dir || !looks_like_id || installed.contains(id.as_str()) {
                continue;
            }
            let path = entry.path();
            let Ok(m) = measure(&path, &MeasureOptions::default(), inodes, cancel) else {
                continue;
            };
            items.push(ProbeItem {
                key: id.clone(),
                name: Some(id),
                bytes: Some(m.allocated),
                paths: vec![path],
                root: Some(root.clone()),
                own_mtime: m.own_mtime,
            });
        }
        items.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(items)
    }
}

/// `Archived and active journals take up 3.9G in the file system.`
/// journalctl's units are powers of 1024.
pub fn parse_journal_usage(output: &str) -> Option<u64> {
    let size = output
        .split_whitespace()
        .skip_while(|w| *w != "up")
        .nth(1)?;
    let unit_len = size
        .chars()
        .rev()
        .take_while(char::is_ascii_alphabetic)
        .count();
    let (num, unit) = size.split_at(size.len() - unit_len);
    let value: f64 = num.parse().ok()?;
    let mult: f64 = match unit {
        "" | "B" => 1.0,
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let bytes = (value * mult).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    bytes.is_finite().then(|| bytes.max(0.0) as u64)
}

/// `(name, revision)` for every row of `snap list --all` marked disabled.
/// Names and revisions that don't look like snap's are ignored rather than
/// passed on.
pub fn parse_disabled_snaps(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            let (name, revision, notes) = (cols.first()?, cols.get(2)?, cols.last()?);
            let name_ok = !name.starts_with('-')
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            let revision_ok = !revision.is_empty() && revision.chars().all(|c| c.is_ascii_digit());
            (notes.split(',').any(|n| n == "disabled") && name_ok && revision_ok)
                .then(|| ((*name).to_string(), (*revision).to_string()))
        })
        .collect()
}

fn first_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).find(|l| !l.is_empty())
}

/// Docker prints decimal sizes like `1.23GB`, `512kB`, `0B`, optionally
/// followed by a percentage: `1.1GB (47%)`. It sometimes reports a negative
/// reclaimable size in scientific notation (`-2.162e+08B`); that means
/// nothing is reclaimable.
pub fn parse_docker_size(s: &str) -> Option<u64> {
    let s = s.split_whitespace().next()?;
    let unit_len = s
        .chars()
        .rev()
        .take_while(char::is_ascii_alphabetic)
        .count();
    let (num, unit) = s.split_at(s.len() - unit_len);
    let value: f64 = num.parse().ok()?;
    let mult: f64 = match unit.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return None,
    };
    let bytes = (value * mult).round();
    if !bytes.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(bytes.max(0.0) as u64)
}

/// `com.apple.TimeMachine.2024-05-01-123456.local` → `2024-05-01-123456`.
pub fn parse_snapshot_dates(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| l.trim().strip_prefix("com.apple.TimeMachine."))
        .filter_map(|l| l.strip_suffix(".local"))
        .filter(|d| d.chars().all(|c| c.is_ascii_digit() || c == '-'))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_docker_size("0B"), Some(0));
        assert_eq!(parse_docker_size("512kB"), Some(512_000));
        assert_eq!(parse_docker_size("1.23GB (47%)"), Some(1_230_000_000));
        assert_eq!(parse_docker_size("2TB"), Some(2_000_000_000_000));
        assert_eq!(parse_docker_size("-2.162e+08B (-7%)"), Some(0));
        assert_eq!(parse_docker_size("1.5e+09B"), Some(1_500_000_000));
        assert_eq!(parse_docker_size("lots"), None);
    }

    #[test]
    fn parses_journal_usage_in_binary_units() {
        let out = "Archived and active journals take up 3.9G in the file system.\n";
        assert_eq!(parse_journal_usage(out), Some(4_187_593_114));
        assert_eq!(
            parse_journal_usage("Archived and active journals take up 512.0M in the file system."),
            Some(536_870_912)
        );
        assert_eq!(parse_journal_usage("Journals take up 8B."), None);
        assert_eq!(parse_journal_usage("No journal files were found."), None);
    }

    #[test]
    fn finds_disabled_snap_revisions_and_nothing_odd() {
        let out = "Name      Version   Rev    Tracking       Publisher   Notes
core20    20230207  1828   latest/stable  canonical✓  base,disabled
core20    20240111  2182   latest/stable  canonical✓  base
firefox   120.0-2   3504   latest/stable  mozilla✓    disabled
firefox   121.0-1   3600   latest/stable  mozilla✓    -
--purge   1         12     latest/stable  x           disabled
evil      1         12;rm  latest/stable  x           disabled
";
        assert_eq!(
            parse_disabled_snaps(out),
            vec![
                ("core20".to_string(), "1828".to_string()),
                ("firefox".to_string(), "3504".to_string()),
            ]
        );
    }

    #[test]
    fn parses_snapshot_dates() {
        let out = "Snapshots for disk /:\ncom.apple.TimeMachine.2024-05-01-123456.local\ncom.apple.TimeMachine.bad;rm.local\n";
        assert_eq!(parse_snapshot_dates(out), vec!["2024-05-01-123456"]);
    }
}
