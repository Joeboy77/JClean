//! Fakes shared by the integration tests. Nothing here runs a real tool,
//! touches the real Trash, or reads the real process list.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use jclean_core::cleaner::Trasher;
use jclean_core::env::Env;
use jclean_core::safety::ProcessChecker;
use jclean_core::tools::{CommandError, CommandOutput, CommandRunner};

/// Tools it "has", with canned output; records every call.
#[derive(Default)]
pub struct FakeRunner {
    pub tools: HashMap<String, CommandOutput>,
    pub calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
}

impl FakeRunner {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn with_tool(mut self, name: &str, stdout: &str) -> Self {
        self.tools.insert(
            name.to_string(),
            CommandOutput {
                status: Some(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
            },
        );
        self
    }

    pub fn calls(&self) -> Vec<(PathBuf, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for FakeRunner {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        _timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        self.calls
            .lock()
            .unwrap()
            .push((program.to_path_buf(), args.to_vec()));
        let name = program.file_name().unwrap().to_string_lossy().into_owned();
        Ok(self.tools.get(&name).cloned().unwrap_or(CommandOutput {
            status: Some(1),
            stdout: String::new(),
            stderr: "not faked".to_string(),
        }))
    }

    fn find_tool(&self, _env: &Env, name: &str) -> Option<PathBuf> {
        self.tools
            .contains_key(name)
            .then(|| PathBuf::from("/fake/bin").join(name))
    }
}

/// Moves "trashed" items into a folder inside the test's temp dir.
pub struct FakeTrash {
    pub dir: PathBuf,
    pub trashed: Mutex<Vec<PathBuf>>,
}

impl FakeTrash {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            trashed: Mutex::new(Vec::new()),
        }
    }
}

impl Trasher for FakeTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        let mut trashed = self.trashed.lock().unwrap();
        let dest = self.dir.join(format!(
            "{}-{}",
            trashed.len(),
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::rename(path, &dest).map_err(|e| e.to_string())?;
        trashed.push(path.to_path_buf());
        Ok(())
    }
}

/// Pretends the named processes are running.
pub struct FakeProcesses(pub Vec<String>);

impl ProcessChecker for FakeProcesses {
    fn running(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|n| self.0.contains(n))
            .cloned()
            .collect()
    }
}
