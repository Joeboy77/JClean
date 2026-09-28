//! Rule tests (spec §16): every rule file validates against the schema, IDs
//! are unique, both label sets and both description fields are present, and
//! user rules are held to the custom-rule limits.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use jclean_core::env::{Env, Os};
use jclean_core::rules::{self, Audience, Detect, Method, RuleError, RuleSet, RuleSource};

#[test]
fn builtin_macos_rules_load_and_validate() {
    let set = RuleSet::builtin(Os::Macos).expect("built-in rules are valid");
    assert!(
        set.rules().len() >= 60,
        "expected the full macOS catalog, got {}",
        set.rules().len()
    );

    let mut ids = HashSet::new();
    for rule in set.rules() {
        assert!(ids.insert(rule.id.clone()), "duplicate id {}", rule.id);
        assert!(
            rule.id.starts_with("macos."),
            "{} should be a macOS rule",
            rule.id
        );
        assert!(
            !rule.labels.developer.trim().is_empty() && !rule.labels.everyday.trim().is_empty(),
            "{} labels",
            rule.id
        );
        assert!(
            !rule.description.what.trim().is_empty()
                && !rule.description.if_cleared.trim().is_empty(),
            "{} description",
            rule.id
        );
        assert_eq!(rule.source, RuleSource::Builtin);
        // Everyday copy never uses the word "junk" (spec §5.6).
        for text in [
            &rule.labels.everyday,
            &rule.description.what,
            &rule.description.if_cleared,
        ] {
            assert!(!text.to_lowercase().contains("junk"), "{}: {text}", rule.id);
        }
    }
}

#[test]
fn every_rule_file_matches_the_schema() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules");
    let mut checked = 0;
    for os in ["macos", "windows", "linux"] {
        let Ok(entries) = std::fs::read_dir(dir.join(os)) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let json = std::fs::read_to_string(&path).unwrap();
                rules::parse_file(&path.display().to_string(), &json, RuleSource::Builtin)
                    .unwrap_or_else(|e| panic!("{e}"));
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn everyday_mode_hides_developer_rules() {
    let set = RuleSet::builtin(Os::Macos).unwrap();
    let everyday: Vec<_> = set.active(Os::Macos, Audience::Everyday).collect();
    let developer: Vec<_> = set.active(Os::Macos, Audience::Developer).collect();
    assert!(
        everyday
            .iter()
            .all(|r| r.audience.contains(&Audience::Everyday))
    );
    assert!(everyday.iter().all(|r| r.id != "macos.node.npm-cache"));
    assert!(developer.len() > everyday.len());
}

#[test]
fn schema_rejects_unknown_fields_and_bad_values() {
    let bad = r#"[{"id":"macos.x.y","version":1,"platforms":["macos"],"audience":["developer"],"category":"developer",
        "group":"g","labels":{"developer":"d","everyday":"e"},"description":{"what":"w","ifCleared":"i"},
        "icon":"i","risk":"extreme","regenerates":true,"detect":{"kind":"fixed","paths":["{home}/x"]},
        "cleanup":{"method":"delete"}}]"#;
    assert!(matches!(
        rules::parse_file("bad.json", bad, RuleSource::Builtin),
        Err(RuleError::Schema { .. })
    ));

    let extra = bad
        .replace("\"extreme\"", "\"safe\"")
        .replace("\"icon\":\"i\"", "\"icon\":\"i\",\"surprise\":1");
    assert!(matches!(
        rules::parse_file("extra.json", &extra, RuleSource::Builtin),
        Err(RuleError::Schema { .. })
    ));
}

fn custom(method: &str, path: &str) -> String {
    format!(
        r#"[{{"id":"macos.custom.renders","version":1,"platforms":["macos"],"audience":["everyday"],"category":"other",
        "group":"Custom","labels":{{"developer":"Renders","everyday":"Renders"}},
        "description":{{"what":"Old renders","ifCleared":"They move to the Trash"}},"icon":"folder","risk":"review",
        "regenerates":false,"detect":{{"kind":"fixed","paths":["{path}"]}},"cleanup":{{"method":"{method}"}}}}]"#
    )
}

#[test]
fn custom_rules_only_trash_and_never_target_protected_paths() {
    let env = Env::new("/fx/Users/me", "/fx", Os::Macos);

    let ok =
        rules::load_custom("pack.json", &custom("trash", "{home}/Movies/Renders"), &env).unwrap();
    assert_eq!(ok[0].source, RuleSource::Custom);
    assert_eq!(ok[0].cleanup.method, Method::Trash);

    for method in ["delete", "command"] {
        assert!(
            rules::load_custom("pack.json", &custom(method, "{home}/Movies/Renders"), &env)
                .is_err(),
            "{method}"
        );
    }
    for protected in [
        "{home}",
        "{home}/Documents",
        "{home}/.ssh/keys",
        "/System/Library",
        "{home}/Library",
    ] {
        assert!(
            rules::load_custom("pack.json", &custom("trash", protected), &env).is_err(),
            "{protected} must be refused"
        );
    }
}

#[test]
fn only_builtin_rules_may_run_commands() {
    let set = RuleSet::builtin(Os::Macos).unwrap();
    let with_commands: Vec<_> = set
        .rules()
        .iter()
        .filter(|r| r.cleanup.method == Method::Command)
        .collect();
    assert!(!with_commands.is_empty());
    assert!(
        with_commands
            .iter()
            .all(|r| r.source == RuleSource::Builtin)
    );
}

#[test]
fn caution_items_are_never_deleted_permanently() {
    let set = RuleSet::builtin(Os::Macos).unwrap();
    for rule in set
        .rules()
        .iter()
        .filter(|r| r.risk == rules::Risk::Caution)
    {
        assert_ne!(rule.cleanup.method, Method::Delete, "{}", rule.id);
        assert_ne!(rule.cleanup.fallback, Some(Method::Delete), "{}", rule.id);
    }
}

#[test]
fn probes_named_in_rules_exist() {
    let set = RuleSet::builtin(Os::Macos).unwrap();
    for rule in set.rules() {
        if let Detect::Probe { probe } = &rule.detect {
            assert!(
                jclean_core::probes::KNOWN_PROBES.contains(&probe.as_str()),
                "{}",
                rule.id
            );
        }
    }
}

#[test]
fn custom_folders_become_trash_only_rules_and_protected_paths_are_refused() {
    let env = Env::new("/fx/Users/me", "/fx", Os::Macos);
    let ok = rules::custom_folder_rule(
        "Old Renders",
        "Old renders",
        std::path::Path::new("/fx/Users/me/Movies/Renders"),
        rules::Risk::Review,
        &env,
    )
    .unwrap();
    assert_eq!(ok.id, "macos.custom.old-renders");
    assert_eq!(ok.cleanup.method, Method::Trash);
    assert_eq!(ok.source, RuleSource::Custom);

    for bad in [
        "/fx/Users/me",
        "/fx/Users/me/Documents",
        "/fx/Users/me/.ssh",
        "/fx/System/Library",
        "relative",
    ] {
        assert!(
            rules::custom_folder_rule(
                "x",
                "x",
                std::path::Path::new(bad),
                rules::Risk::Review,
                &env
            )
            .is_err(),
            "{bad} must be refused"
        );
    }
    assert!(
        rules::custom_folder_rule(
            "x",
            "x",
            std::path::Path::new("/fx/Users/me/Movies/R"),
            rules::Risk::Info,
            &env
        )
        .is_err()
    );
}
