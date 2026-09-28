//! Keep policies: hold back the newest versions of versioned items such as
//! IDE caches, device support files and editor extensions.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::model::{Keep, KeepGroup};

/// Compares strings treating digit runs as numbers: "1.10" > "1.9".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    loop {
        match (a.first(), b.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let da = a.iter().take_while(|c| c.is_ascii_digit()).count();
                let db = b.iter().take_while(|c| c.is_ascii_digit()).count();
                let (na, nb) = (trim_zeros(&a[..da]), trim_zeros(&b[..db]));
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if ord != Ordering::Equal {
                    return ord;
                }
                a = &a[da..];
                b = &b[db..];
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(y);
                }
                a = &a[1..];
                b = &b[1..];
            }
        }
    }
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let start = digits
        .iter()
        .position(|&c| c != b'0')
        .unwrap_or(digits.len());
    &digits[start..]
}

/// The name without its version, or `None` if it has no recognisable version.
///
/// `ms-python.python-2024.2.1` → `ms-python.python`,
/// `IntelliJIdea2024.2` → `IntelliJIdea`, `gradle-8.5-bin` → `gradle`.
/// A leading digit (`42crunch.vscode-openapi-4.1`) is never taken as the version.
pub fn version_group(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    // A separator followed by an optional `v` and a digit starts the version.
    for i in 1..bytes.len() {
        if matches!(bytes[i - 1], b'-' | b'_' | b' ') {
            let rest = &bytes[i..];
            let digit_at = usize::from(rest.first() == Some(&b'v'));
            if rest.get(digit_at).is_some_and(u8::is_ascii_digit) && i >= 2 {
                return Some(&name[..i - 1]);
            }
        }
    }
    // Otherwise a letter-to-digit boundary, but only if the rest is purely a version.
    for i in 1..bytes.len() {
        if bytes[i - 1].is_ascii_alphabetic()
            && bytes[i].is_ascii_digit()
            && bytes[i..].iter().all(|c| c.is_ascii_digit() || *c == b'.')
        {
            return Some(&name[..i]);
        }
    }
    None
}

/// Splits candidates into those to clean and those the policy keeps.
pub fn apply(paths: Vec<PathBuf>, keep: &Keep) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut ungrouped = Vec::new();
    for path in paths {
        let name = file_name(&path);
        let key = match keep.group_by {
            KeepGroup::Parent => Some(
                path.parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            KeepGroup::Name => version_group(&name).map(|g| {
                let parent = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!("{parent}\u{0}{g}")
            }),
        };
        match key {
            Some(key) => groups.entry(key).or_default().push(path),
            // Unversioned items have nothing newer to fall back on, so they're kept.
            None => ungrouped.push(path),
        }
    }

    let newest = usize::try_from(keep.newest).unwrap_or(usize::MAX);
    let mut clean = Vec::new();
    let mut kept = ungrouped;
    for (_, mut members) in groups {
        members.sort_by(|a, b| natural_cmp(&file_name(b), &file_name(a)));
        let rest = members.split_off(newest.min(members.len()));
        kept.extend(members);
        clean.extend(rest);
    }
    clean.sort();
    kept.sort();
    (clean, kept)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_numbers() {
        assert_eq!(natural_cmp("1.10", "1.9"), Ordering::Greater);
        assert_eq!(
            natural_cmp("PyCharm2024.1", "PyCharm2023.3"),
            Ordering::Greater
        );
        assert_eq!(natural_cmp("17.2 (21C62)", "17.10 (21A1)"), Ordering::Less);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    }

    #[test]
    fn version_groups() {
        assert_eq!(
            version_group("ms-python.python-2024.2.1"),
            Some("ms-python.python")
        );
        assert_eq!(
            version_group("ms-vscode.cpptools-1.2.3-darwin-arm64"),
            Some("ms-vscode.cpptools")
        );
        assert_eq!(
            version_group("42crunch.vscode-openapi-4.25.3"),
            Some("42crunch.vscode-openapi")
        );
        assert_eq!(version_group("IntelliJIdea2024.2"), Some("IntelliJIdea"));
        assert_eq!(version_group("gradle-8.5-bin"), Some("gradle"));
        assert_eq!(version_group("k8s-tools"), None);
        assert_eq!(version_group("vscode-pdf"), None);
        assert_eq!(version_group("17.2 (21C62)"), None);
    }

    #[test]
    fn keeps_newest_per_name_group() {
        let base = PathBuf::from("/x");
        let paths = [
            "PyCharm2023.3",
            "PyCharm2024.1",
            "IntelliJIdea2024.2",
            "IntelliJIdea2023.1",
            "shared",
        ]
        .iter()
        .map(|n| base.join(n))
        .collect();
        let (clean, kept) = apply(
            paths,
            &Keep {
                newest: 1,
                group_by: KeepGroup::Name,
            },
        );
        assert_eq!(
            clean,
            vec![base.join("IntelliJIdea2023.1"), base.join("PyCharm2023.3")]
        );
        assert_eq!(
            kept,
            vec![
                base.join("IntelliJIdea2024.2"),
                base.join("PyCharm2024.1"),
                base.join("shared")
            ]
        );
    }

    #[test]
    fn keeps_newest_per_parent() {
        let paths = vec![
            PathBuf::from("/d/16.4"),
            PathBuf::from("/d/17.10"),
            PathBuf::from("/d/17.2"),
        ];
        let (clean, kept) = apply(
            paths,
            &Keep {
                newest: 1,
                group_by: KeepGroup::Parent,
            },
        );
        assert_eq!(kept, vec![PathBuf::from("/d/17.10")]);
        assert_eq!(
            clean,
            vec![PathBuf::from("/d/16.4"), PathBuf::from("/d/17.2")]
        );
    }
}
