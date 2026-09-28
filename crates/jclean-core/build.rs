//! Embeds every rule file under `rules/<os>/` into the binary, so adding a
//! rule never needs a Rust change.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let rules_dir = manifest.join("../../rules");
    println!("cargo:rerun-if-changed={}", rules_dir.display());

    let mut out = String::from("pub static BUILTIN_RULE_FILES: &[(&str, &str, &str)] = &[\n");
    for os in ["macos", "windows", "linux"] {
        let dir = rules_dir.join(os);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for file in files {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let abs = fs::canonicalize(&file).unwrap_or(file.clone());
            let _ = writeln!(
                out,
                "    ({os:?}, {name:?}, include_str!({:?})),",
                abs.display().to_string()
            );
        }
    }
    out.push_str("];\n");

    let dest = PathBuf::from(env::var("OUT_DIR").unwrap_or_default()).join("builtin_rules.rs");
    if let Err(err) = fs::write(&dest, out) {
        panic!("couldn't write {}: {err}", dest.display());
    }
}
