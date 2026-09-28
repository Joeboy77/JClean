//! JClean's core engine: scanning, sizing, rules, planning, safety and cleaning.
//!
//! This crate has no Tauri dependency so it can be tested in isolation and
//! driven by `jclean-cli`. See `docs/SPEC.md` §3 for the module layout.

/// Version of the core engine, taken from the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
