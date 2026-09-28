use std::path::{Path, PathBuf};

use super::{DEV_NAMES, ProtectedPath, Scope};
use crate::env::Env;
use crate::rules::Category;

/// A folder from an environment variable, or `fallback` when it's unset or
/// not absolute (as in tests, which run against a fixture root).
fn var_dir(env: &Env, name: &str, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    env.var(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(fallback)
}

fn local_app_data(env: &Env) -> PathBuf {
    var_dir(env, "LOCALAPPDATA", || env.home().join("AppData/Local"))
}

fn app_data(env: &Env) -> PathBuf {
    var_dir(env, "APPDATA", || env.home().join("AppData/Roaming"))
}

fn system_root(env: &Env) -> PathBuf {
    var_dir(env, "SystemRoot", || env.root().join("Windows"))
}

fn program_files(env: &Env) -> Vec<PathBuf> {
    vec![
        var_dir(env, "ProgramFiles", || env.root().join("Program Files")),
        var_dir(env, "ProgramFiles(x86)", || {
            env.root().join("Program Files (x86)")
        }),
    ]
}

pub(super) fn known_dir(env: &Env, token: &str) -> Option<PathBuf> {
    match token {
        "home" => Some(env.home().to_path_buf()),
        // Spec §6.2: `{caches}` is %LOCALAPPDATA% on Windows.
        "caches" | "localAppData" => Some(local_app_data(env)),
        "appData" | "appSupport" => Some(app_data(env)),
        "temp" => env
            .var("TEMP")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute()),
        _ => None,
    }
}

/// Top-level folders of the user profile that are never cleaned themselves.
const KNOWN_FOLDERS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Videos",
    "Favorites",
    "Contacts",
    "Links",
    "Saved Games",
    "Searches",
    "3D Objects",
    "AppData",
    "AppData/Local",
    "AppData/Roaming",
    "AppData/LocalLow",
];

/// Where Windows keeps keys and saved passwords: the equivalent of the Keychain.
const CREDENTIAL_STORES: &[&str] = &[
    "AppData/Roaming/Microsoft/Protect",
    "AppData/Roaming/Microsoft/Credentials",
    "AppData/Local/Microsoft/Credentials",
    "AppData/Roaming/Microsoft/Crypto",
    "AppData/Roaming/Microsoft/SystemCertificates",
    "AppData/Local/Microsoft/Vault",
];

pub(super) fn protected_paths(env: &Env) -> Vec<ProtectedPath> {
    let home = |p: &str| env.home().join(p);
    let exact = |path| ProtectedPath {
        path,
        scope: Scope::Exact,
    };
    let subtree = |path| ProtectedPath {
        path,
        scope: Scope::Subtree,
    };
    let windows = system_root(env);

    let mut paths = vec![
        exact(env.root().to_path_buf()),
        subtree(windows.clone()),
        // The one cleanup target inside the Windows folder (spec §8.3).
        ProtectedPath {
            path: windows.join("SoftwareDistribution/Download"),
            scope: Scope::CleanInside,
        },
        exact(var_dir(env, "ProgramData", || {
            env.root().join("ProgramData")
        })),
        exact(env.root().join("$Recycle.Bin")),
        exact(env.home().to_path_buf()),
        subtree(home(".ssh")),
        subtree(home(".gnupg")),
        subtree(home(".aws")),
        subtree(home(".kube")),
        exact(home(".config")),
    ];
    if let Some(users) = env.home().parent() {
        paths.push(exact(users.to_path_buf()));
    }
    paths.extend(program_files(env).into_iter().map(subtree));
    paths.extend(KNOWN_FOLDERS.iter().map(|f| exact(home(f))));
    paths.extend(CREDENTIAL_STORES.iter().map(|f| subtree(home(f))));
    paths.extend(cloud_dirs(env).into_iter().map(exact));
    paths
}

/// OneDrive (including "OneDrive - Company"), iCloud Drive, Dropbox and Box
/// keep files as cloud placeholders. Reading them would download them.
pub(super) fn cloud_dirs(env: &Env) -> Vec<PathBuf> {
    let home = env.home();
    let mut dirs: Vec<PathBuf> = ["iCloudDrive", "Pictures/iCloud Photos", "Dropbox", "Box"]
        .iter()
        .map(|d| home.join(d))
        .collect();
    if let Ok(entries) = std::fs::read_dir(home) {
        let mut onedrive: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("OneDrive"))
            .map(|e| e.path())
            .collect();
        onedrive.sort();
        dirs.extend(onedrive);
    }
    if let Some(d) = env
        .var("OneDrive")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        && !dirs.contains(&d)
    {
        dirs.push(d);
    }
    dirs
}

pub(super) fn default_scan_exclusions(env: &Env) -> Vec<PathBuf> {
    let mut paths = vec![env.home().join("AppData")];
    paths.extend(cloud_dirs(env));
    paths
}

pub(super) fn tool_dirs(env: &Env) -> Vec<PathBuf> {
    let home = env.home();
    let windows = system_root(env);
    let [program_files, _] = <[PathBuf; 2]>::try_from(program_files(env))
        .unwrap_or_else(|_| [env.root().join("Program Files"), PathBuf::new()]);
    let local = local_app_data(env);
    let mut dirs = vec![
        windows.join("System32"),
        windows.join("System32/WindowsPowerShell/v1.0"),
        // nvm-windows points NVM_SYMLINK at the active Node version.
        var_dir(env, "NVM_SYMLINK", || program_files.join("nodejs")),
        program_files.join("nodejs"),
        app_data(env).join("npm"),
        local.join("pnpm"),
        local.join("Volta/bin"),
        home.join(".bun/bin"),
        home.join(".cargo/bin"),
        program_files.join("Go/bin"),
        home.join("go/bin"),
        program_files.join("Docker/Docker/resources/bin"),
        program_files.join("dotnet"),
        home.join("scoop/shims"),
        local.join("Microsoft/WinGet/Links"),
    ];
    dirs.dedup();
    dirs
}

/// Machine-specific data (history, size cache), so not the roaming profile.
pub(super) fn app_data_dir(env: &Env) -> PathBuf {
    local_app_data(env).join("app.jclean")
}

pub(super) fn categorize(env: &Env, path: &Path, parent: Category) -> Category {
    let Ok(rel) = path.strip_prefix(env.home()) else {
        return if program_files(env).iter().any(|p| path.starts_with(p)) {
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
        (Some(top), None) if top.starts_with("OneDrive") || top == "iCloudDrive" => {
            Category::Documents
        }
        (Some("AppData"), Some("Local" | "Roaming" | "LocalLow")) => Category::Apps,
        (Some("AppData"), None) => Category::System,
        _ => parent,
    }
}
