//! CLAUDE.md safety rule 1: all deletion goes through `cleaner::execute`.
//! Nothing else in the Rust codebase may remove files or use the Trash.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &["remove_dir_all", "remove_file", "remove_dir(", "trash::"];
const ALLOWED: &str = "crates/jclean-core/src/cleaner.rs";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() && name != "target" && name != "node_modules" && name != "gen" {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_cleaner_deletes() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&repo.join("crates"), &mut files);
    rust_files(&repo.join("apps"), &mut files);
    assert!(files.len() > 10);

    let mut offenders = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&repo)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == ALLOWED || rel.ends_with("tests/architecture.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            if FORBIDDEN.iter().any(|f| line.contains(f)) {
                offenders.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "deletion outside cleaner.rs:\n{}",
        offenders.join("\n")
    );
}
