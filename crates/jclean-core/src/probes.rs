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
            })
            .collect())
    }
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
    fn parses_snapshot_dates() {
        let out = "Snapshots for disk /:\ncom.apple.TimeMachine.2024-05-01-123456.local\ncom.apple.TimeMachine.bad;rm.local\n";
        assert_eq!(parse_snapshot_dates(out), vec!["2024-05-01-123456"]);
    }
}
