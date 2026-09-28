use std::path::{Path, PathBuf};

use super::{ProtectedPath, Scope};
use crate::env::Env;
use crate::rules::Category;

pub(super) fn known_dir(env: &Env, token: &str) -> Option<PathBuf> {
    let home = env.home();
    match token {
        "home" => Some(home.to_path_buf()),
        "caches" => Some(home.join("Library/Caches")),
        "appSupport" => Some(home.join("Library/Application Support")),
        "logs" => Some(home.join("Library/Logs")),
        "temp" => env
            .var("TMPDIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute()),
        _ => None,
    }
}

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

    let mut paths = vec![
        exact(env.root().to_path_buf()),
        subtree(sys("/System")),
        // Homebrew's cache under /usr/local is cleared by `brew`, never by path.
        subtree(sys("/usr")),
        subtree(sys("/bin")),
        subtree(sys("/sbin")),
        subtree(sys("/etc")),
        subtree(sys("/private/etc")),
        subtree(sys("/private/var/db")),
        // Installer apps are a v2 exception (spec §7.2).
        subtree(sys("/Applications")),
        exact(sys("/Library")),
        exact(sys("/Users")),
        exact(env.home().to_path_buf()),
        subtree(home(".ssh")),
        subtree(home(".gnupg")),
        subtree(home(".aws")),
        subtree(home(".kube")),
        exact(home(".config")),
        subtree(home("Library/Keychains")),
        subtree(home("Library/Mail")),
    ];
    for top in [
        "Documents",
        "Desktop",
        "Pictures",
        "Music",
        "Movies",
        "Downloads",
        "Library",
    ] {
        paths.push(exact(home(top)));
    }
    paths
}

pub(super) fn cloud_dirs(env: &Env) -> Vec<PathBuf> {
    vec![
        env.home().join("Library/Mobile Documents"),
        env.home().join("Library/CloudStorage"),
    ]
}

pub(super) fn default_scan_exclusions(env: &Env) -> Vec<PathBuf> {
    let mut paths = vec![env.home().join("Library"), env.home().join(".Trash")];
    paths.extend(cloud_dirs(env));
    paths
}

pub(super) fn tool_dirs(env: &Env) -> Vec<PathBuf> {
    let home = env.home();
    let mut dirs = vec![
        env.system_path(Path::new("/opt/homebrew/bin")),
        env.system_path(Path::new("/usr/local/bin")),
        env.system_path(Path::new("/usr/bin")),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join(".bun/bin"),
        home.join(".volta/bin"),
        home.join(".local/bin"),
        env.system_path(Path::new("/usr/local/go/bin")),
        env.system_path(Path::new("/Applications/Docker.app/Contents/Resources/bin")),
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

pub(super) fn app_data_dir(env: &Env) -> PathBuf {
    env.home().join("Library/Application Support/app.jclean")
}

/// Folders that are developer storage wherever they appear.
const DEV_NAMES: &[&str] = &[
    "node_modules",
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".bun",
    ".nvm",
    ".volta",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    ".android",
    ".pub-cache",
    ".docker",
    ".cache",
    "go",
    "Developer",
    "target",
    ".venv",
    "venv",
    "DerivedData",
    "CoreSimulator",
];

pub(super) fn categorize(env: &Env, path: &Path, parent: Category) -> Category {
    let Ok(rel) = path.strip_prefix(env.home()) else {
        return if path.starts_with(env.system_path(Path::new("/Applications"))) {
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
        (Some("Pictures" | "Movies" | "Music"), None) => Category::Media,
        (Some("Documents" | "Desktop" | "Downloads"), None) => Category::Documents,
        (Some("Library"), Some("Mobile Documents" | "CloudStorage")) => Category::Documents,
        (
            Some("Library"),
            Some("Caches" | "Application Support" | "Containers" | "Group Containers" | "Logs"),
        ) => Category::Apps,
        (Some("Library"), None) => Category::System,
        (Some(".Trash"), None) => Category::Other,
        _ => parent,
    }
}
