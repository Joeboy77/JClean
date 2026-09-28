//! The environment a scan runs in: home directory, filesystem root, OS and
//! environment variables. Everything that touches the real machine goes
//! through [`Env`], so tests can point it at a fixture instead.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("couldn't find the home directory")]
    NoHome,
}

#[derive(Debug, Clone)]
pub struct Env {
    home: PathBuf,
    root: PathBuf,
    os: Os,
    vars: HashMap<String, OsString>,
}

impl Env {
    /// The real machine. Only binaries call this; tests use [`Env::new`].
    pub fn from_system() -> Result<Self, EnvError> {
        let vars: HashMap<String, OsString> = std::env::vars_os()
            .filter_map(|(k, v)| k.into_string().ok().map(|k| (k, v)))
            .collect();
        let home = vars
            .get(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(EnvError::NoHome)?;
        // System paths in rules are relative to the system drive on Windows.
        let root = if cfg!(windows) {
            let drive = vars
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("SystemDrive"))
                .map_or_else(|| "C:".into(), |(_, v)| v.to_string_lossy().into_owned());
            PathBuf::from(format!("{drive}\\"))
        } else {
            PathBuf::from("/")
        };
        Ok(Self {
            home,
            root,
            os: Os::current(),
            vars,
        })
    }

    /// An environment rooted somewhere else, typically a fixture directory.
    /// System paths such as `/Library/Logs` resolve under `root`.
    pub fn new(home: impl Into<PathBuf>, root: impl Into<PathBuf>, os: Os) -> Self {
        Self {
            home: home.into(),
            root: root.into(),
            os,
            vars: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_var(mut self, name: &str, value: impl Into<OsString>) -> Self {
        self.vars.insert(name.to_string(), value.into());
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn os(&self) -> Os {
        self.os
    }

    /// An environment variable. Names are case-insensitive on Windows, as
    /// they are there (`SystemRoot` and `SYSTEMROOT` are the same).
    pub fn var(&self, name: &str) -> Option<&OsStr> {
        self.vars
            .get(name)
            .or_else(|| {
                (self.os == Os::Windows)
                    .then(|| {
                        self.vars
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(name))
                            .map(|(_, v)| v)
                    })
                    .flatten()
            })
            .map(OsString::as_os_str)
    }

    /// Maps an absolute system path (as written in a rule) under the root.
    /// With the real root `/` this is the identity.
    pub fn system_path(&self, abs: &Path) -> PathBuf {
        let rel: PathBuf = abs
            .components()
            .filter(|c| !matches!(c, Component::RootDir | Component::Prefix(_)))
            .collect();
        self.root.join(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_paths_resolve_under_root() {
        let env = Env::new("/fx/Users/me", "/fx", Os::Macos);
        assert_eq!(
            env.system_path(Path::new("/Library/Logs")),
            PathBuf::from("/fx/Library/Logs")
        );
        let real = Env::new("/Users/me", "/", Os::Macos);
        assert_eq!(
            real.system_path(Path::new("/Library/Logs")),
            PathBuf::from("/Library/Logs")
        );
    }
}
