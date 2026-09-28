//! The full scan's folder map (spec §5.2). The UI asks for one level at a
//! time (spec §15), so the whole tree never has to cross into the frontend.
//! (The quick-scan map of what was found is built in the UI from the items
//! it already has.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::disktree::TreeNode;
use crate::env::Env;
use crate::platform;
use crate::rules::Category;
use crate::scanner::{ScanItem, ScanResult};

/// Most cells shown at one level; the smallest merge into one (spec §5.2).
pub const MAX_CELLS: usize = 150;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCell {
    /// Pass back to [`children`] to drill in: `fs:<absolute path>`.
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub category: Category,
    /// Bytes inside this cell that could be cleaned.
    pub reclaimable: u64,
    /// Set when the cell is exactly one list item (for row ↔ cell linking).
    pub item_id: Option<String>,
    /// The cell can be drilled into.
    pub has_children: bool,
    /// The merged "Other small items" cell.
    pub other: bool,
}

/// The root level's ID.
pub const ROOT: &str = "";

/// Cells one level below `id`, largest first, capped at [`MAX_CELLS`].
/// `None` if the ID doesn't exist in this scan.
/// `None` if the scan has no folder tree (a quick scan) or the ID isn't in it.
pub fn children(scan: &ScanResult, env: &Env, id: &str) -> Option<Vec<MapCell>> {
    if scan.tree.is_empty() {
        return None;
    }
    Some(cap(tree_children(scan, env, id)?))
}

/// Largest first; everything past the limit merges into one cell.
fn cap(mut cells: Vec<MapCell>) -> Vec<MapCell> {
    cells.retain(|c| c.bytes > 0);
    cells.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    if cells.len() <= MAX_CELLS {
        return cells;
    }
    let rest = cells.split_off(MAX_CELLS - 1);
    cells.push(MapCell {
        id: "other".to_string(),
        name: format!("Other small items ({})", rest.len()),
        bytes: rest.iter().map(|c| c.bytes).sum(),
        category: Category::Other,
        reclaimable: rest.iter().map(|c| c.reclaimable).sum(),
        item_id: None,
        has_children: false,
        other: true,
    });
    cells
}

fn reclaimable(item: &ScanItem) -> u64 {
    if item.cleanable { item.bytes } else { 0 }
}

/// The real folder tree from a full scan.
fn tree_children(scan: &ScanResult, env: &Env, id: &str) -> Option<Vec<MapCell>> {
    // Reclaimable bytes and items by path, so every folder knows what it holds.
    let mut rec_under: HashMap<PathBuf, u64> = HashMap::new();
    let mut item_at: HashMap<&Path, &str> = HashMap::new();
    for item in &scan.items {
        let Some(path) = &item.path else { continue };
        item_at.insert(path.as_path(), item.id.as_str());
        let r = reclaimable(item);
        if r > 0 {
            for a in path.ancestors() {
                *rec_under.entry(a.to_path_buf()).or_default() += r;
            }
        }
    }

    let (node, path, parent_cat) = if id == ROOT {
        // One cell per scan root; with the usual single root, show its contents.
        match scan.tree.as_slice() {
            [only] => {
                let path = PathBuf::from(&only.name);
                (only, path, Category::Other)
            }
            roots => {
                return Some(
                    roots
                        .iter()
                        .map(|r| {
                            let p = PathBuf::from(&r.name);
                            cell_for(r, &p, env, Category::Other, &rec_under, &item_at)
                        })
                        .collect(),
                );
            }
        }
    } else {
        let target = PathBuf::from(id.strip_prefix("fs:")?);
        find(scan, env, &target)?
    };

    Some(
        node.children
            .iter()
            .map(|child| {
                let p = path.join(&child.name);
                cell_for(child, &p, env, parent_cat, &rec_under, &item_at)
            })
            .collect(),
    )
}

fn cell_for(
    node: &TreeNode,
    path: &Path,
    env: &Env,
    parent: Category,
    rec_under: &HashMap<PathBuf, u64>,
    item_at: &HashMap<&Path, &str>,
) -> MapCell {
    MapCell {
        id: format!("fs:{}", path.display()),
        name: node.name.clone(),
        bytes: node.allocated,
        category: platform::categorize(env, path, parent),
        reclaimable: rec_under
            .get(path)
            .copied()
            .unwrap_or(0)
            .min(node.allocated),
        item_id: item_at.get(path).map(|s| (*s).to_string()),
        has_children: node.is_dir && !node.children.is_empty(),
        other: false,
    }
}

/// Walks from a scan root down to `target`, tracking the inherited category.
fn find<'a>(
    scan: &'a ScanResult,
    env: &Env,
    target: &Path,
) -> Option<(&'a TreeNode, PathBuf, Category)> {
    let root = scan.tree.iter().find(|r| target.starts_with(&r.name))?;
    let mut path = PathBuf::from(&root.name);
    let mut node = root;
    let mut cat = Category::Other;
    for comp in target.strip_prefix(&root.name).ok()?.components() {
        let name = comp.as_os_str().to_string_lossy();
        node = node.children.iter().find(|c| c.name == name)?;
        path.push(&*name);
        cat = platform::categorize(env, &path, cat);
    }
    Some((node, path, cat))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(name: &str, bytes: u64) -> MapCell {
        MapCell {
            id: name.to_string(),
            name: name.to_string(),
            bytes,
            category: Category::Other,
            reclaimable: 0,
            item_id: None,
            has_children: false,
            other: false,
        }
    }

    #[test]
    fn caps_cells_and_merges_the_smallest() {
        let cells: Vec<MapCell> = (1..=200).map(|i| cell(&i.to_string(), i)).collect();
        let capped = cap(cells);
        assert_eq!(capped.len(), MAX_CELLS);
        assert_eq!(capped[0].bytes, 200);
        let other = capped.last().unwrap();
        assert!(other.other);
        assert_eq!(other.bytes, (1..=51).sum::<u64>());
    }
}
