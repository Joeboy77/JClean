//! Running external tools (spec §7.4): resolved from a fixed list of
//! folders, spawned with an argument vector (never a shell), with a timeout.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::env::{Env, Os};
use crate::platform;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// `None` if the process was killed by a signal.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("couldn't start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} didn't finish within {} seconds", timeout.as_secs())]
    Timeout { program: String, timeout: Duration },
    #[error("{program} failed: {source}")]
    Wait {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// Runs programs. Tests substitute a fake so no real tool is ever invoked.
pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError>;

    /// Finds a tool by name in the platform's known folders.
    fn find_tool(&self, env: &Env, name: &str) -> Option<PathBuf> {
        find_tool(env, name)
    }
}

pub fn find_tool(env: &Env, name: &str) -> Option<PathBuf> {
    if name.contains(['/', '\\']) {
        return None;
    }
    // Windows tools are `docker.exe`, but also `npm.cmd` and `yarn.cmd`.
    let names: Vec<String> = if env.os() == Os::Windows && !name.contains('.') {
        [".exe", ".cmd", ".bat"]
            .iter()
            .map(|ext| format!("{name}{ext}"))
            .collect()
    } else {
        vec![name.to_string()]
    };
    platform::tool_dirs(env)
        .into_iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

/// Spawns real processes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        let name = program.display().to_string();
        let mut command = Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // A GUI app's child would otherwise open a console window.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| CommandError::Spawn {
                program: name.clone(),
                source,
            })?;

        // Drain pipes on their own threads so a chatty tool can't block on a full pipe.
        let drain = |pipe: Option<Box<dyn Read + Send>>| {
            thread::spawn(move || {
                let mut buf = Vec::new();
                if let Some(mut p) = pipe {
                    let _ = p.read_to_end(&mut buf);
                }
                String::from_utf8_lossy(&buf).into_owned()
            })
        };
        let out = drain(
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let err = drain(
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CommandError::Timeout {
                        program: name,
                        timeout,
                    });
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(source) => {
                    return Err(CommandError::Wait {
                        program: name,
                        source,
                    });
                }
            }
        };

        Ok(CommandOutput {
            status: status.code(),
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn runs_without_a_shell_and_captures_output() {
        let out = SystemRunner
            .run(
                Path::new("/bin/echo"),
                &["hello; rm -rf /".to_string()],
                DEFAULT_TIMEOUT,
            )
            .unwrap();
        assert!(out.success());
        assert_eq!(out.stdout.trim(), "hello; rm -rf /");
    }

    #[test]
    fn enforces_the_timeout() {
        let err = SystemRunner
            .run(
                Path::new("/bin/sleep"),
                &["5".to_string()],
                Duration::from_millis(100),
            )
            .unwrap_err();
        assert!(matches!(err, CommandError::Timeout { .. }));
    }
}
