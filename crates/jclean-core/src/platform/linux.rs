use std::path::{Path, PathBuf};

use super::{DEV_NAMES, ProtectedPath, Scope};
use crate::env::Env;
use crate::rules::Category;

/// An XDG base directory: the variable when it's set to an absolute path
/// (the spec says relative values are invalid), otherwise its default.
fn xdg(env: &Env, name: &str, default: &str) -> PathBuf {
    env.var(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| env.home().join(default))
}

fn cache_home(env: &Env) -> PathBuf {
    xdg(env, "XDG_CACHE_HOME", ".cache")
}

fn data_home(env: &Env) -> PathBuf {
    xdg(env, "XDG_DATA_HOME", ".local/share")
}

fn config_home(env: &Env) -> PathBuf {
    xdg(env, "XDG_CONFIG_HOME", ".config")
}

fn state_home(env: &Env) -> PathBuf {
    xdg(env, "XDG_STATE_HOME", ".local/state")
}

pub(super) fn known_dir(env: &Env, token: &str) -> Option<PathBuf> {
    match token {
        "home" => Some(env.home().to_path_buf()),
        "caches" => Some(cache_home(env)),
        "config" => Some(config_home(env)),
        "data" | "appSupport" => Some(data_home(env)),
        "state" => Some(state_home(env)),
        "temp" => Some(
            env.var("TMPDIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| env.system_path(Path::new("/tmp"))),
        ),
        _ => None,
    }
}

/// The folders an XDG desktop creates in every home.
const USER_DIRS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Music",
    "Pictures",
    "Videos",
    "Templates",
    "Public",
];

pub(super) fn protected_paths(env: &Env) -> Vec<ProtectedPath> {
    let sys = |p: &str| env.system_path(Path::new(p));
    let home = |p: &str| env.home().join(p);
    let exact = |path| ProtectedPath {
        path,
        scope: Scope::Exact,
    };
    let subtree = |path| ProtectedPath {
        path,
        scope: Scope::Subtree,
    };

    let mut paths = vec![exact(env.root().to_path_buf())];
    // The operating system. Package caches under /var are cleared by the
    // package manager's own command, never by path.
    for dir in [
        "/bin", "/boot", "/dev", "/etc", "/lib", "/lib32", "/lib64", "/libx32", "/opt", "/proc",
        "/root", "/run", "/sbin", "/snap", "/srv", "/sys", "/usr", "/var",
    ] {
        paths.push(subtree(sys(dir)));
    }
    // Package caches the package manager clears with its own command
    // (spec §8, phase 9).
    for dir in [
        "/var/cache/apt/archives",
        "/var/cache/dnf",
        "/var/cache/libdnf5",
    ] {
        paths.push(ProtectedPath {
            path: sys(dir),
            scope: Scope::CleanInside,
        });
    }
    paths.push(exact(sys("/home")));
    paths.push(exact(env.home().to_path_buf()));
    paths.extend(USER_DIRS.iter().map(|d| exact(home(d))));
    // Keys, passwords and certificates: the equivalent of the Keychain.
    for dir in [
        ".ssh",
        ".gnupg",
        ".aws",
        ".kube",
        ".pki",
        ".password-store",
        ".mozilla",
        ".thunderbird",
    ] {
        paths.push(subtree(home(dir)));
    }
    paths.push(subtree(data_home(env).join("keyrings")));
    for dir in [
        cache_home(env),
        config_home(env),
        data_home(env),
        state_home(env),
        home(".local"),
        home("snap"),
        home(".var"),
        home(".var/app"),
    ] {
        paths.push(exact(dir));
    }
    paths.extend(cloud_dirs(env).into_iter().map(exact));
    paths
}

/// Synced folders. Their contents are never walked for projects or the map.
pub(super) fn cloud_dirs(env: &Env) -> Vec<PathBuf> {
    ["Dropbox", "pCloudDrive", "OneDrive", "Nextcloud"]
        .iter()
        .map(|d| env.home().join(d))
        .collect()
}

pub(super) fn default_scan_exclusions(env: &Env) -> Vec<PathBuf> {
    let mut paths = vec![
        cache_home(env),
        env.home().join(".local"),
        env.home().join(".var"),
        env.home().join("snap"),
        data_home(env).join("Trash"),
    ];
    paths.extend(cloud_dirs(env));
    paths
}

pub(super) fn tool_dirs(env: &Env) -> Vec<PathBuf> {
    let home = env.home();
    let mut dirs = vec![
        env.system_path(Path::new("/usr/local/bin")),
        env.system_path(Path::new("/usr/bin")),
        env.system_path(Path::new("/bin")),
        env.system_path(Path::new("/usr/sbin")),
        env.system_path(Path::new("/snap/bin")),
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        env.system_path(Path::new("/usr/local/go/bin")),
        home.join(".bun/bin"),
        home.join(".volta/bin"),
        data_home(env).join("pnpm"),
        env.system_path(Path::new("/home/linuxbrew/.linuxbrew/bin")),
        home.join(".linuxbrew/bin"),
    ];
    // nvm installs one bin directory per Node version; newest first.
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path().join("bin"))
            .collect();
        versions.sort_by(|a, b| {
            crate::rules::keep::natural_cmp(&b.to_string_lossy(), &a.to_string_lossy())
        });
        dirs.extend(versions);
    }
    dirs
}

/// `~/.local/share/app.jclean`, where Tauri keeps the app's data too.
pub(super) fn app_data_dir(env: &Env) -> PathBuf {
    data_home(env).join("app.jclean")
}

pub(super) fn categorize(env: &Env, path: &Path, parent: Category) -> Category {
    let Ok(rel) = path.strip_prefix(env.home()) else {
        return if path.starts_with(env.system_path(Path::new("/opt")))
            || path.starts_with(env.system_path(Path::new("/snap")))
        {
            Category::Apps
        } else {
            Category::System
        };
    };
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if DEV_NAMES.contains(&name) {
        return Category::Developer;
    }
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or_default());
    match (parts.next(), parts.next()) {
        (Some("Pictures" | "Videos" | "Music"), None) => Category::Media,
        (Some("Documents" | "Desktop" | "Downloads"), None) => Category::Documents,
        (Some(".cache" | ".config" | ".var" | "snap"), None) => Category::Apps,
        (Some(".local"), Some("share")) => Category::Apps,
        (Some(".local"), None) => Category::System,
        _ => parent,
    }
}
