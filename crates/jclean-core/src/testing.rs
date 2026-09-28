//! Fixture homes for tests and `jclean-cli fixture` (spec §16). A fixture is
//! a fake filesystem root with a home folder inside it; point an [`Env`] at
//! it and nothing ever touches the real home.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use filetime::FileTime;

use crate::env::{Env, Os};

pub struct Fixture {
    pub root: PathBuf,
    pub home: PathBuf,
}

impl Fixture {
    /// An empty fixture inside `base` (which should be a temporary folder).
    pub fn new(base: &Path) -> std::io::Result<Self> {
        let root = base.join("root");
        let home = root.join("Users/tester");
        fs::create_dir_all(&home)?;
        // Canonical paths keep comparisons simple on macOS, where the
        // temporary folder lives behind the /var -> /private/var symlink.
        let root = fs::canonicalize(&root)?;
        let home = fs::canonicalize(&home)?;
        Ok(Self { root, home })
    }

    pub fn env(&self) -> Env {
        Env::new(&self.home, &self.root, Os::Macos)
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }

    /// Writes a file of `bytes` real bytes under the home folder.
    pub fn file(&self, rel: &str, bytes: usize) -> std::io::Result<PathBuf> {
        let path = self.path(rel);
        write_file(&path, bytes)?;
        Ok(path)
    }

    /// Writes a file under the fixture root (a "system" location).
    pub fn system_file(&self, rel: &str, bytes: usize) -> std::io::Result<PathBuf> {
        let path = self.root.join(rel);
        write_file(&path, bytes)?;
        Ok(path)
    }

    pub fn dir(&self, rel: &str) -> std::io::Result<PathBuf> {
        let path = self.path(rel);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// On Windows this needs Developer Mode or an administrator (as on CI).
    pub fn symlink(&self, rel: &str, target: &Path) -> std::io::Result<PathBuf> {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, &path)?;
        #[cfg(windows)]
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, &path)?;
        } else {
            std::os::windows::fs::symlink_file(target, &path)?;
        }
        Ok(path)
    }

    /// A directory junction (Windows): the link type that doesn't need any
    /// privileges, and the one the cleaner must never follow.
    #[cfg(windows)]
    pub fn junction(&self, rel: &str, target: &Path) -> std::io::Result<PathBuf> {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let status = std::process::Command::new("cmd")
            .arg("/d")
            .arg("/c")
            .arg("mklink")
            .arg("/J")
            .arg(&path)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(path)
        } else {
            Err(std::io::Error::other("mklink /J failed"))
        }
    }

    pub fn hard_link(&self, existing: &str, rel: &str) -> std::io::Result<PathBuf> {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::hard_link(self.path(existing), &path)?;
        Ok(path)
    }

    /// Sets the modification time of everything under `rel` (and `rel`
    /// itself) to `days` ago. Symlinks aren't followed.
    pub fn age(&self, rel: &str, days: u64) -> std::io::Result<()> {
        age_tree(&self.path(rel), days)
    }

    /// A home with one of everything the scanner and guard care about.
    pub fn standard(base: &Path) -> std::io::Result<Self> {
        let f = Self::new(base)?;
        const MB: usize = 1_000_000;

        // Package caches.
        f.file(".npm/_cacache/content-v2/sha512/aa/one", 3 * MB)?;
        f.file(".npm/_cacache/index-v5/idx", 50_000)?;
        f.file("Library/Caches/Yarn/v6/pkg.tgz", 2 * MB)?;

        // App caches, including ones the guard must refuse.
        f.file("Library/Caches/com.example.editor/Cache.db", MB)?;
        f.file("Library/Caches/com.apple.Safari/leave-alone.db", MB)?;
        f.file("Library/Caches/com.example.keepme/data.bin", MB)?;
        f.file("Library/Caches/com.example.keepme/.jclean-keep", 0)?;
        f.file("Documents/important.txt", 10_000)?;
        #[cfg(unix)]
        f.symlink(
            "Library/Caches/com.example.editor/escape",
            &f.path("Documents"),
        )?;
        let ro = f.file("Library/Caches/com.example.editor/readonly.bin", 200_000)?;
        let mut perms = fs::metadata(&ro)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro, perms)?;

        // Xcode.
        f.file(
            "Library/Developer/Xcode/DerivedData/MyApp-abc123/Build/Products/app.o",
            5 * MB,
        )?;

        // Logs.
        f.file("Library/Logs/DiagnosticReports/Thing.crash", 100_000)?;
        f.file("Library/Logs/SomeApp/app.log", 200_000)?;

        // Trash and Downloads.
        f.file(".Trash/old-photo.jpg", 500_000)?;
        f.file("Downloads/Setup.dmg", 4 * MB)?;
        f.file("Downloads/fresh.pkg", MB)?;
        f.file("Downloads/notes.txt", 10_000)?;

        // pnpm store with a file hard-linked into a project.
        f.file("Library/pnpm/store/v3/files/aa/lodash", 2 * MB)?;

        // An old website: inactive, so node_modules is pre-selected.
        f.file("code/old-site/package.json", 500)?;
        f.file("code/old-site/index.js", 2_000)?;
        f.file("code/old-site/node_modules/react/index.js", 6 * MB)?;
        f.file("code/old-site/dist/bundle.js", MB)?;

        // An app worked on today: node_modules is only `review`.
        f.file("code/active-app/package.json", 500)?;
        f.file("code/active-app/src/main.ts", 2_000)?;
        f.file("code/active-app/node_modules/vite/index.js", 3 * MB)?;
        f.hard_link(
            "Library/pnpm/store/v3/files/aa/lodash",
            "code/active-app/node_modules/.pnpm/lodash/index.js",
        )?;

        // An old Rust project.
        f.file("code/rusty/Cargo.toml", 300)?;
        f.file("code/rusty/src/main.rs", 1_000)?;
        f.file("code/rusty/target/debug/rusty", 4 * MB)?;

        // A monorepo whose package files are old but whose repo is active.
        f.file("code/mono/.git/logs/HEAD", 1_000)?;
        f.file("code/mono/packages/web/package.json", 500)?;
        f.file("code/mono/packages/web/node_modules/pkg/index.js", MB)?;

        // Everything starts old; the active bits are made fresh below.
        age_tree(&f.home, 200)?;
        age_tree(&f.path("Downloads/fresh.pkg"), 1)?;
        for fresh in [
            "code/active-app/package.json",
            "code/active-app/src/main.ts",
            "code/mono/.git/logs/HEAD",
        ] {
            age_tree(&f.path(fresh), 0)?;
        }
        Ok(f)
    }
}

fn write_file(path: &Path, bytes: usize) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    // Non-zero bytes so no filesystem stores the file sparsely.
    let chunk = vec![0x5a_u8; 64 * 1024];
    let mut left = bytes;
    while left > 0 {
        let n = left.min(chunk.len());
        file.write_all(&chunk[..n])?;
        left -= n;
    }
    file.sync_all()
}

fn age_tree(path: &Path, days: u64) -> std::io::Result<()> {
    let when = SystemTime::now() - Duration::from_secs(days * 86_400);
    let time = FileTime::from_system_time(when);
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            age_tree(&entry?.path(), days)?;
        }
    }
    filetime::set_symlink_file_times(path, time, time)
}
