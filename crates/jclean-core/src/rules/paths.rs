//! Rule path patterns: token substitution (`{home}`, `{caches}`, `{env:NAME}`…)
//! and glob expansion one path component at a time.

use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobMatcher};

use crate::env::Env;
use crate::platform;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathError {
    #[error("unknown path token {{{0}}}")]
    UnknownToken(String),
    #[error("unclosed path token in {0:?}")]
    Unclosed(String),
    #[error("path must be absolute or start with a token: {0:?}")]
    Relative(String),
    #[error("path may not contain '..': {0:?}")]
    ParentComponent(String),
    #[error("invalid glob {0:?}: {1}")]
    Glob(String, String),
}

const TOKENS: &[&str] = &[
    "home",
    "caches",
    "appSupport",
    "logs",
    "temp",
    "localAppData",
    "appData",
];

/// Substitutes tokens. `Ok(None)` means a token has no value here (for
/// example `{localAppData}` on macOS), so the path doesn't apply.
pub fn resolve(pattern: &str, env: &Env) -> Result<Option<PathBuf>, PathError> {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| PathError::Unclosed(pattern.to_string()))?;
        let token = &after[..end];
        out.push_str(&rest[..start]);
        if let Some(name) = token.strip_prefix("env:") {
            match env.var(name) {
                Some(value) => out.push_str(&value.to_string_lossy()),
                None => return Ok(None),
            }
        } else if TOKENS.contains(&token) {
            match platform::known_dir(env, token) {
                Some(dir) => out.push_str(&dir.to_string_lossy()),
                None => return Ok(None),
            }
        } else if token.contains(',') {
            // Glob alternation such as `{cache,src}`; left for the glob matcher.
            out.push('{');
            out.push_str(token);
            out.push('}');
        } else {
            return Err(PathError::UnknownToken(token.to_string()));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);

    let path = PathBuf::from(&out);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PathError::ParentComponent(pattern.to_string()));
    }
    if !path.is_absolute() {
        return Err(PathError::Relative(pattern.to_string()));
    }
    // Literal system paths (no leading token) live under the environment root.
    if pattern.starts_with('{') {
        Ok(Some(path))
    } else {
        Ok(Some(env.system_path(&path)))
    }
}

fn has_glob(s: &str) -> bool {
    s.contains(['*', '?', '[', '{'])
}

/// The part of a pattern before its first glob component.
pub fn literal_prefix(pattern: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in pattern.components() {
        if has_glob(&comp.as_os_str().to_string_lossy()) {
            break;
        }
        out.push(comp);
    }
    out
}

pub fn matcher(glob: &str) -> Result<GlobMatcher, PathError> {
    Glob::new(glob)
        .map(|g| g.compile_matcher())
        .map_err(|e| PathError::Glob(glob.to_string(), e.to_string()))
}

/// Expands a resolved pattern into existing paths, sorted. Intermediate
/// folders must be real directories: symlinks are never followed.
pub fn expand(pattern: &Path) -> Result<Vec<PathBuf>, PathError> {
    let prefix = literal_prefix(pattern);
    let rest: Vec<String> = pattern
        .components()
        .skip(prefix.components().count())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();

    if rest.is_empty() {
        return Ok(if fs::symlink_metadata(&prefix).is_ok() {
            vec![prefix]
        } else {
            Vec::new()
        });
    }
    if !is_real_dir(&prefix) {
        return Ok(Vec::new());
    }

    let mut current = vec![prefix];
    for (i, comp) in rest.iter().enumerate() {
        let last = i + 1 == rest.len();
        let mut next = Vec::new();
        if has_glob(comp) {
            let m = matcher(comp)?;
            for dir in &current {
                let Ok(entries) = fs::read_dir(dir) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok) {
                    let name = entry.file_name();
                    if m.is_match(&name) {
                        let path = entry.path();
                        if last || is_real_dir(&path) {
                            next.push(path);
                        }
                    }
                }
            }
        } else {
            for dir in &current {
                let path = dir.join(comp);
                let exists = if last {
                    fs::symlink_metadata(&path).is_ok()
                } else {
                    is_real_dir(&path)
                };
                if exists {
                    next.push(path);
                }
            }
        }
        current = next;
    }
    current.sort();
    Ok(current)
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Os;

    fn env() -> Env {
        Env::new("/fx/Users/me", "/fx", Os::Macos).with_var("GOPATH", "/fx/Users/me/go")
    }

    #[test]
    fn resolves_tokens() {
        let e = env();
        assert_eq!(
            resolve("{home}/.npm/_cacache", &e),
            Ok(Some(PathBuf::from("/fx/Users/me/.npm/_cacache")))
        );
        assert_eq!(
            resolve("{caches}/Yarn", &e),
            Ok(Some(PathBuf::from("/fx/Users/me/Library/Caches/Yarn")))
        );
        assert_eq!(
            resolve("{env:GOPATH}/pkg/mod", &e),
            Ok(Some(PathBuf::from("/fx/Users/me/go/pkg/mod")))
        );
        assert_eq!(resolve("{env:MISSING}/x", &e), Ok(None));
        assert_eq!(resolve("{localAppData}/x", &e), Ok(None));
        assert_eq!(
            resolve("/Library/Logs", &e),
            Ok(Some(PathBuf::from("/fx/Library/Logs")))
        );
        assert_eq!(
            resolve("{home}/.cargo/registry/{cache,src}", &e),
            Ok(Some(PathBuf::from(
                "/fx/Users/me/.cargo/registry/{cache,src}"
            )))
        );
    }

    #[test]
    fn rejects_bad_patterns() {
        let e = env();
        assert!(matches!(
            resolve("{nope}/x", &e),
            Err(PathError::UnknownToken(_))
        ));
        assert!(matches!(
            resolve("relative/x", &e),
            Err(PathError::Relative(_))
        ));
        assert!(matches!(
            resolve("{home}/../etc", &e),
            Err(PathError::ParentComponent(_))
        ));
        assert!(matches!(
            resolve("{home/x", &e),
            Err(PathError::Unclosed(_))
        ));
    }

    #[test]
    fn literal_prefix_stops_at_first_glob() {
        assert_eq!(literal_prefix(Path::new("/a/b/*/c")), PathBuf::from("/a/b"));
        assert_eq!(literal_prefix(Path::new("/a/{x,y}")), PathBuf::from("/a"));
        assert_eq!(literal_prefix(Path::new("/a/b")), PathBuf::from("/a/b"));
    }
}
