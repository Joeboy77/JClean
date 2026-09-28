//! Rule file format (spec §6.1, §6.2). Field names match the JSON.

use serde::{Deserialize, Serialize};

use crate::env::Os;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    /// `<os>.<ecosystem>.<name>`
    pub id: String,
    pub version: u32,
    pub platforms: Vec<Os>,
    pub audience: Vec<Audience>,
    pub category: Category,
    pub group: String,
    pub labels: Labels,
    pub description: Description,
    pub icon: String,
    pub risk: Risk,
    pub regenerates: bool,
    pub detect: Detect,
    #[serde(default)]
    pub unused: Unused,
    pub cleanup: Cleanup,
    /// Process names that should be closed before cleaning.
    #[serde(default)]
    pub related_apps: Vec<String>,
    /// Sizes may be overstated because of APFS clones; the UI says "up to".
    #[serde(default)]
    pub may_share_blocks: bool,
    /// Allows measuring and cleaning across a mount point. Off by default.
    #[serde(default)]
    pub cross_filesystems: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<Keep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// Built in or user supplied. Never read from JSON.
    #[serde(skip)]
    pub source: RuleSource,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuleSource {
    #[default]
    Builtin,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    Everyday,
    Developer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Apps,
    Developer,
    System,
    Media,
    Documents,
    Other,
}

/// Ordered from least to most careful, so `max` picks the stricter one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Review,
    Caution,
    Info,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Review => "review",
            Self::Caution => "caution",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub developer: String,
    pub everyday: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Description {
    pub what: String,
    pub if_cleared: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Detect {
    /// Known paths, globs allowed. With `eachChild`, every entry inside a
    /// matched folder becomes its own item and the folder itself is kept.
    Fixed {
        paths: Vec<String>,
        #[serde(default)]
        each_child: bool,
        /// Child names (globs) to leave out when `eachChild` is set.
        #[serde(default)]
        exclude: Vec<String>,
    },
    /// Build and dependency folders next to a project marker (spec §4.5).
    ProjectArtifact {
        markers: Vec<String>,
        folders: Vec<String>,
    },
    /// Ask a tool (spec §8, e.g. `docker system df`).
    Probe { probe: String },
    /// Files matching a pattern, optionally old or large.
    Query {
        paths: Vec<String>,
        #[serde(default)]
        names: Vec<String>,
        #[serde(default)]
        older_than_days: Option<u32>,
        #[serde(default)]
        min_bytes: Option<u64>,
        /// Search all the way down. Only runs in a full scan.
        #[serde(default)]
        recursive: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Unused {
    pub basis: UnusedBasis,
    #[serde(default)]
    pub min_age_days: u32,
}

impl Default for Unused {
    fn default() -> Self {
        Self {
            basis: UnusedBasis::NewestMtime,
            min_age_days: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnusedBasis {
    /// Newest modification time inside the location (spec §4.4, 2).
    NewestMtime,
    /// Project's last commit or source edit (spec §4.4, 1).
    ProjectActivity,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Permanent.
    Delete,
    Trash,
    /// The tool's own cleanup command.
    Command,
    None,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Trash => "trash",
            Self::Command => "command",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cleanup {
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<ToolCommand>,
    /// Used when the tool isn't installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Method>,
    #[serde(default)]
    pub requires_admin: bool,
    /// Clear the folder's contents but keep the folder itself.
    #[serde(default)]
    pub keep_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCommand {
    pub tool: String,
    /// `{item}` is replaced by the item's key: its folder name, or the probe's key.
    pub args: Vec<String>,
}

/// Keep the newest versions of each item (spec §6.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keep {
    pub newest: u32,
    #[serde(default)]
    pub group_by: KeepGroup,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeepGroup {
    /// All items under the same folder form one group.
    #[default]
    Parent,
    /// Items group by name with the version removed, e.g. `PyCharm2024.1`
    /// and `PyCharm2024.2` are one group.
    Name,
}
