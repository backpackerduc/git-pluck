use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::Command;

use anyhow::{Context, bail};

use crate::map_modify::{MapEntry, MapTarget};

/// A single entry from `git ls-tree` output.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub mode: String,
    pub obj_type: String,
    pub sha: String,
    pub path: String,
}

/// Dump all blobs and trees from a commit via `git ls-tree -r -t --full-tree`.
///
/// Returns entries sorted by path.
pub fn dump_tree(revision: &str) -> anyhow::Result<Vec<TreeEntry>> {
    let output = Command::new("git")
        .args(["ls-tree", "-r", "-t", "--full-tree", revision])
        .output()
        .context("Failed to run git ls-tree")?;

    if !output.status.success() {
        bail!("git ls-tree failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(entry) = parse_ls_tree_line(line) {
            entries.push(entry);
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Parse a single `git ls-tree` line into a `TreeEntry`.
///
/// Expected format: `<mode> <type> <sha>\t<path>`
fn parse_ls_tree_line(line: &str) -> Option<TreeEntry> {
    let tab_pos = line.find('\t')?;
    let meta = &line[..tab_pos];
    let path = line[tab_pos + 1..].to_string();

    let parts: Vec<&str> = meta.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    Some(TreeEntry { mode: parts[0].to_string(), obj_type: parts[1].to_string(), sha: parts[2].to_string(), path })
}

/// Build the pluck tree for a given source commit.
///
/// Maps each entry from the full tree dump according to the pluck map,
/// handling mirror, remove, unpack, move, and copy targets.
/// Entries under the root base prefix (`.` mapping) get prefixed accordingly.
pub fn build_pluck_tree(
    source_sha: &str,
    map_entries: &[MapEntry],
    config: &crate::config::PluckConfig,
) -> anyhow::Result<Vec<TreeEntry>> {
    let full_tree = dump_tree(source_sha)?;
    let mut seen_sources: BTreeSet<String> = BTreeSet::new();
    let mut emplaced: BTreeSet<String> = BTreeSet::new();

    let base_prefix = get_base_prefix(map_entries);
    let dot_mirror = has_dot_mirror(map_entries);

    let mut pluck_entries: BTreeMap<String, TreeEntry> = BTreeMap::new();

    for entry in &full_tree {
        if let Some(mapped) = find_mapping(&entry.path, map_entries) {
            seen_sources.insert(mapped.source_key.clone());
            process_entry(
                entry,
                &mapped,
                base_prefix.as_ref(),
                &mut pluck_entries,
                &mut emplaced,
                &full_tree,
                map_entries,
            );
        } else if let Some(ref prefix) = base_prefix {
            if !is_overridden(&entry.path, map_entries) {
                let new_path = format!("{}/{}", prefix, entry.path);
                pluck_entries.insert(
                    new_path.clone(),
                    TreeEntry {
                        mode: entry.mode.clone(),
                        obj_type: entry.obj_type.clone(),
                        sha: entry.sha.clone(),
                        path: new_path,
                    },
                );
            }
        } else if dot_mirror {
            pluck_entries.insert(entry.path.clone(), entry.clone());
        }
    }

    // Mark the root source as seen if base prefix or dot mirror was active
    if base_prefix.is_some() || dot_mirror {
        seen_sources.insert(".".to_string());
    }

    // Check for missing mapped sources
    for map_entry in map_entries {
        if !seen_sources.contains(&map_entry.src_path) {
            if !config.allow_missing_path && !config.force {
                bail!("Mapped source '{}' not found in commit {}", map_entry.src_path, source_sha);
            }
            eprintln!("Warning: mapped source '{}' not found in commit {}", map_entry.src_path, source_sha);
        }
    }

    // Remove entries marked as emplaced parent directories
    pluck_entries.retain(|path, _| !emplaced.contains(path.as_str()));

    let mut result: Vec<TreeEntry> = pluck_entries.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(result)
}

/// Internal representation of a matched mapping for a tree entry.
struct MappedSource {
    source_key: String,
    target: MapTarget,
    copies: Vec<String>,
}

/// Find the first map entry that matches the given path.
///
/// A match occurs when the path equals the source or is a descendant.
fn find_mapping(path: &str, map_entries: &[MapEntry]) -> Option<MappedSource> {
    for entry in map_entries {
        if path == entry.src_path || path.starts_with(&format!("{}/", &entry.src_path)) {
            return Some(MappedSource {
                source_key: entry.src_path.clone(),
                target: entry.primary.clone(),
                copies: entry.copies.clone(),
            });
        }
    }
    None
}

/// Check if a path has an explicit mapping override.
///
/// The `.` (root) source is removed since it is the catch-all, not an override.
fn is_overridden(path: &str, map_entries: &[MapEntry]) -> bool {
    map_entries.iter().any(|entry| {
        entry.src_path != "." && (path == entry.src_path || path.starts_with(&format!("{}/", &entry.src_path)))
    })
}

/// Get the root base prefix if the map contains `[forward.from "."] to = <dest>`.
fn get_base_prefix(map_entries: &[MapEntry]) -> Option<String> {
    for entry in map_entries {
        if entry.src_path == "."
            && let MapTarget::Move(ref dst) = entry.primary
            && dst != "."
        {
            return Some(dst.clone());
        }
    }
    None
}

/// Check if the map contains `[forward.from "."] to = (Mirror)` (mirror mapping for root).
fn has_dot_mirror(map_entries: &[MapEntry]) -> bool {
    for entry in map_entries {
        if entry.src_path == "." && matches!(entry.primary, MapTarget::Mirror) {
            return true;
        }
    }
    false
}

/// Process a single tree entry according to its mapped target.
///
/// Handles mirror, remove, unpack, move, and copy destinations.
/// For remove, emplaces parent directories to preserve siblings.
fn process_entry(
    entry: &TreeEntry,
    mapped: &MappedSource,
    base_prefix: Option<&String>,
    pluck_entries: &mut BTreeMap<String, TreeEntry>,
    emplaced: &mut BTreeSet<String>,
    full_tree: &[TreeEntry],
    map_entries: &[MapEntry],
) {
    match &mapped.target {
        MapTarget::Mirror => {
            pluck_entries.insert(entry.path.clone(), entry.clone());
        }
        MapTarget::Remove => {
            emplace_parent_trees(&entry.path, pluck_entries, emplaced, full_tree, base_prefix);
        }
        MapTarget::Root => {
            unpack_tree(entry, pluck_entries, base_prefix);
        }
        MapTarget::Move(dst) => {
            let new_path = resolve_move_path(&entry.path, &mapped.source_key, dst, base_prefix, map_entries);
            pluck_entries.insert(
                new_path.clone(),
                TreeEntry {
                    mode: entry.mode.clone(),
                    obj_type: entry.obj_type.clone(),
                    sha: entry.sha.clone(),
                    path: new_path,
                },
            );
        }
    }

    // Handle copy entries
    for copy_dst in &mapped.copies {
        let copy_path = resolve_move_path(&entry.path, &mapped.source_key, copy_dst, base_prefix, map_entries);
        pluck_entries.insert(
            copy_path.clone(),
            TreeEntry {
                mode: entry.mode.clone(),
                obj_type: entry.obj_type.clone(),
                sha: entry.sha.clone(),
                path: copy_path,
            },
        );
    }
}

/// Resolve the destination path for a move or copy.
///
/// Strips the source prefix from the original path and prepends the destination.
/// If a root base prefix is active and the result doesn't already start with it, prefixes it.
/// Skips base prefix if the source has an explicit non-root mapping (override).
fn resolve_move_path(
    original_path: &str,
    source_key: &str,
    dst: &str,
    base_prefix: Option<&String>,
    map_entries: &[MapEntry],
) -> String {
    let suffix =
        if original_path == source_key { String::new() } else { original_path[source_key.len() + 1..].to_string() };

    let path = if suffix.is_empty() { dst.to_string() } else { format!("{dst}/{suffix}") };

    if let Some(prefix) = base_prefix
        && !path.starts_with(prefix)
    {
        // Skip base prefix if source has an explicit non-root mapping (override)
        let is_override = map_entries.iter().any(|e| e.src_path == *source_key && e.src_path != ".");
        if !is_override {
            return format!("{prefix}/{path}");
        }
    }

    path
}

/// Unpack a tree entry: list its contents and add each child directly.
///
/// Children are placed at the pluck root or under the base prefix if active.
fn unpack_tree(entry: &TreeEntry, pluck_entries: &mut BTreeMap<String, TreeEntry>, base_prefix: Option<&String>) {
    let output = Command::new("git").args(["ls-tree", "-r", "-t", "--full-tree", &entry.sha]).output();

    let Ok(output) = output else { return };
    if !output.status.success() {
        return;
    }

    let prefix = base_prefix.map_or("", |s| s.as_str());

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(child) = parse_ls_tree_line(line) {
            let new_path = if prefix.is_empty() { child.path } else { format!("{}/{}", prefix, child.path) };
            pluck_entries.insert(
                new_path.clone(),
                TreeEntry { mode: child.mode, obj_type: child.obj_type, sha: child.sha, path: new_path },
            );
        }
    }
}

/// Emplace ancestor directories when a tree/blob is removed or renamed.
///
/// Walks from the path upward, one component at a time. For each ancestor
/// that hasn't been emplaced yet, lists its contents from the source tree
/// and adds each child under the original pluck-space path. Marks both
/// resolved and original paths as emplaced.
fn emplace_parent_trees(
    path: &str,
    pluck_entries: &mut BTreeMap<String, TreeEntry>,
    emplaced: &mut BTreeSet<String>,
    full_tree: &[TreeEntry],
    base_prefix: Option<&String>,
) {
    let mut current = path.to_string();

    while let Some(pos) = current.rfind('/') {
        let parent = &current[..pos];
        if parent.is_empty() {
            break;
        }

        if emplaced.contains(parent) {
            break;
        }

        let resolved_path = resolve_emplace_path(parent, base_prefix);

        let children: Vec<TreeEntry> = full_tree
            .iter()
            .filter(|e| {
                let child_parent = e.path.rfind('/').map_or("", |p| &e.path[..p]);
                child_parent == resolved_path || child_parent == parent
            })
            .map(|e| {
                let new_path = if resolved_path == parent {
                    e.path.clone()
                } else {
                    let suffix = &e.path[resolved_path.len() + 1..];
                    format!("{parent}/{suffix}")
                };
                TreeEntry { mode: e.mode.clone(), obj_type: e.obj_type.clone(), sha: e.sha.clone(), path: new_path }
            })
            .collect();

        for child in children {
            pluck_entries.entry(child.path.clone()).or_insert(child);
        }

        emplaced.insert(resolved_path.to_string());
        emplaced.insert(parent.to_string());

        current = parent.to_string();
    }
}

/// Resolve an emplace path by stripping the root base prefix if active.
fn resolve_emplace_path<'a>(path: &'a str, base_prefix: Option<&String>) -> &'a str {
    if let Some(prefix) = base_prefix
        && path.starts_with(prefix)
    {
        let stripped = &path[prefix.len() + 1..];
        if !stripped.is_empty() {
            return stripped;
        }
    }
    path
}

/// Create Git tree objects from a flat list of entries.
///
/// Groups entries by directory, creates trees bottom-up via `git mktree --batch`,
/// and returns the root tree OID. Returns the empty tree OID for an empty list.
pub fn create_tree_objects(entries: &[TreeEntry]) -> anyhow::Result<git2::Oid> {
    if entries.is_empty() {
        return get_empty_tree_oid();
    }

    // Group entries by their parent directory
    let mut dir_entries: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();

    for entry in entries {
        let parent = entry.path.rfind('/').map_or("", |p| &entry.path[..p]);
        dir_entries.entry(parent.to_string()).or_default().push(entry.clone());
    }

    // Collect all directories that need trees:
    // - directories that have direct entries
    // - parent directories of those directories (up to root)
    let mut dirs_to_create: BTreeSet<String> = dir_entries.keys().cloned().collect();
    for dir in dir_entries.keys() {
        let mut d = dir.clone();
        loop {
            if let Some(pos) = d.rfind('/') {
                d = d[..pos].to_string();
                dirs_to_create.insert(d.clone());
            } else {
                // Reached top-level dir; ensure root is included
                if !d.is_empty() {
                    dirs_to_create.insert(String::new());
                }
                break;
            }
        }
    }

    // Process directories bottom-up (deepest first)
    let mut created_trees: std::collections::HashMap<String, git2::Oid> = std::collections::HashMap::new();

    let sorted_dirs: Vec<String> = dirs_to_create.into_iter().collect();
    for dir in sorted_dirs.iter().rev() {
        let child_entries = dir_entries.get(dir).cloned().unwrap_or_default();

        // Collect child tree OIDs that were already created
        let child_dirs: Vec<&String> = sorted_dirs
            .iter()
            .filter(|d| {
                if dir.is_empty() {
                    // Root level: direct children have no '/' in their path
                    !d.is_empty() && d.find('/').is_none()
                } else {
                    let prefix = format!("{dir}/");
                    d.starts_with(&prefix) && d.len() > dir.len() + 1 && d[dir.len() + 1..].find('/').is_none()
                }
            })
            .collect();

        let mut mktree_lines: Vec<String> = Vec::new();

        // Add blob entries
        for entry in &child_entries {
            if entry.obj_type == "tree" {
                // Skip tree entries from ls-tree; they'll be replaced by child trees
                continue;
            }
            let name = if dir.is_empty() { &entry.path } else { &entry.path[dir.len() + 1..] };
            mktree_lines.push(format!("{} {} {}\t{}", entry.mode, entry.obj_type, entry.sha, name));
        }

        // Add subdirectory tree entries
        for child_dir in &child_dirs {
            if let Some(tree_oid) = created_trees.get(*child_dir) {
                let name = if dir.is_empty() { child_dir.as_str() } else { &child_dir.as_str()[dir.len() + 1..] };
                mktree_lines.push(format!("40000 tree {tree_oid}\t{name}"));
            }
        }

        if mktree_lines.is_empty() {
            // Empty directory - create empty tree
            let oid = get_empty_tree_oid()?;
            created_trees.insert(dir.clone(), oid);
            continue;
        }

        let batch_input = mktree_lines.join("\n") + "\n";
        let output = Command::new("git")
            .args(["mktree", "--batch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn git mktree")?;

        let mut child = output;
        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(batch_input.as_bytes())?;
        }
        drop(child.stdin.take());

        let output_result = child.wait_with_output().context("Failed to wait for git mktree")?;

        if !output_result.status.success() {
            bail!("git mktree failed for dir '{}': {}", dir, String::from_utf8_lossy(&output_result.stderr));
        }

        let stdout = String::from_utf8_lossy(&output_result.stdout);
        if let Some(first_line) = stdout.lines().next() {
            let tree_oid = git2::Oid::from_str(first_line.trim()).context("Failed to parse tree OID from mktree")?;
            created_trees.insert(dir.clone(), tree_oid);
        }
    }

    created_trees.get("").copied().ok_or_else(|| anyhow::anyhow!("Failed to create root tree"))
}

/// Compute the empty tree OID at runtime.
///
/// Uses `git hash-object` so the OID always matches whatever hash
/// algorithm the current Git version uses — no hardcoded constant.
pub fn get_empty_tree_oid() -> anyhow::Result<git2::Oid> {
    let output = Command::new("git")
        .args(["hash-object", "-t", "tree", "/dev/null"])
        .output()
        .context("Failed to run git hash-object")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    git2::Oid::from_str(stdout.trim()).context("Failed to parse empty tree OID")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_ls_tree_line --

    #[test]
    fn test_parse_ls_tree_blob() {
        let line = "100644 blob aaaa1111bbbb2222cccc3333dddd4444eeee5555\tsrc/file.txt";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.mode, "100644");
        assert_eq!(entry.obj_type, "blob");
        assert_eq!(entry.sha, "aaaa1111bbbb2222cccc3333dddd4444eeee5555");
        assert_eq!(entry.path, "src/file.txt");
    }

    #[test]
    fn test_parse_ls_tree_tree() {
        let line = "40000 tree bbbb1111bbbb2222cccc3333dddd4444eeee5555\tsrc";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.mode, "40000");
        assert_eq!(entry.obj_type, "tree");
        assert_eq!(entry.path, "src");
    }

    #[test]
    fn test_parse_ls_tree_executable() {
        let line = "100755 blob aaaa1111bbbb2222cccc3333dddd4444eeee5555\tbuild.sh";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.mode, "100755");
        assert_eq!(entry.path, "build.sh");
    }

    #[test]
    fn test_parse_ls_tree_symlink() {
        let line = "120000 symlink aaaa1111bbbb2222cccc3333dddd4444eeee5555\tlink";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.mode, "120000");
        assert_eq!(entry.obj_type, "symlink");
    }

    #[test]
    fn test_parse_ls_tree_no_tab_returns_none() {
        assert!(parse_ls_tree_line("no tab here").is_none());
    }

    #[test]
    fn test_parse_ls_tree_empty_line_returns_none() {
        assert!(parse_ls_tree_line("").is_none());
    }

    #[test]
    fn test_parse_ls_tree_too_few_parts() {
        assert!(parse_ls_tree_line("only_one\tpath").is_none());
    }

    #[test]
    fn test_parse_ls_tree_path_with_spaces() {
        let line = "100644 blob aaaa1111bbbb2222cccc3333dddd4444eeee5555\tsrc/my file.txt";
        let entry = parse_ls_tree_line(line).unwrap();
        assert_eq!(entry.path, "src/my file.txt");
    }

    // -- get_base_prefix --

    #[test]
    fn test_get_base_prefix_none() {
        let entries = vec![MapEntry {
            src_path: "src".to_string(),
            primary: MapTarget::Move("dest".to_string()),
            copies: vec![],
        }];
        assert!(get_base_prefix(&entries).is_none());
    }

    #[test]
    fn test_get_base_prefix_move() {
        let entries = vec![MapEntry {
            src_path: ".".to_string(),
            primary: MapTarget::Move("backend".to_string()),
            copies: vec![],
        }];
        assert_eq!(get_base_prefix(&entries), Some("backend".to_string()));
    }

    #[test]
    fn test_get_base_prefix_dot_mirror() {
        // [forward.from "."] to = . should not produce a base prefix
        let entries = vec![MapEntry { src_path: ".".to_string(), primary: MapTarget::Root, copies: vec![] }];
        assert!(get_base_prefix(&entries).is_none());
    }

    // -- resolve_move_path --

    #[test]
    fn test_resolve_move_path_exact_match() {
        let path = resolve_move_path("src", "src", "dest", None, &[]);
        assert_eq!(path, "dest");
    }

    #[test]
    fn test_resolve_move_path_nested() {
        let path = resolve_move_path("src/file.txt", "src", "dest", None, &[]);
        assert_eq!(path, "dest/file.txt");
    }

    #[test]
    fn test_resolve_move_path_deep_nested() {
        let path = resolve_move_path("src/a/b/c.txt", "src", "dest", None, &[]);
        assert_eq!(path, "dest/a/b/c.txt");
    }

    #[test]
    fn test_resolve_move_path_with_base_prefix() {
        let path = resolve_move_path("src/file.txt", "src", "dest", Some(&"backend".to_string()), &[]);
        assert_eq!(path, "backend/dest/file.txt");
    }

    #[test]
    fn test_resolve_move_path_already_has_prefix() {
        // If the path already starts with the prefix, don't double-prefix
        let path = resolve_move_path("backend/file.txt", "backend", "backend", Some(&"backend".to_string()), &[]);
        assert_eq!(path, "backend/file.txt");
    }

    // -- find_mapping --

    #[test]
    fn test_find_mapping_exact() {
        let entries = vec![MapEntry {
            src_path: "src".to_string(),
            primary: MapTarget::Move("dest".to_string()),
            copies: vec![],
        }];
        let mapped = find_mapping("src", &entries).unwrap();
        assert_eq!(mapped.source_key, "src");
        assert!(matches!(mapped.target, MapTarget::Move(_)));
    }

    #[test]
    fn test_find_mapping_descendant() {
        let entries = vec![MapEntry {
            src_path: "src".to_string(),
            primary: MapTarget::Move("dest".to_string()),
            copies: vec![],
        }];
        let mapped = find_mapping("src/file.txt", &entries).unwrap();
        assert_eq!(mapped.source_key, "src");
    }

    #[test]
    fn test_find_mapping_no_match() {
        let entries = vec![MapEntry {
            src_path: "src".to_string(),
            primary: MapTarget::Move("dest".to_string()),
            copies: vec![],
        }];
        assert!(find_mapping("other/file.txt", &entries).is_none());
    }

    #[test]
    fn test_find_mapping_prefix_not_partial() {
        // "src" should not match "srcdir" (needs exact or /-separated)
        let entries = vec![MapEntry {
            src_path: "src".to_string(),
            primary: MapTarget::Move("dest".to_string()),
            copies: vec![],
        }];
        assert!(find_mapping("srcdir/file.txt", &entries).is_none());
    }

    // -- is_overridden --

    #[test]
    fn test_is_overridden_exact() {
        let entries = vec![MapEntry { src_path: "docs".to_string(), primary: MapTarget::Remove, copies: vec![] }];
        assert!(is_overridden("docs", &entries));
    }

    #[test]
    fn test_is_overridden_descendant() {
        let entries = vec![MapEntry { src_path: "docs".to_string(), primary: MapTarget::Remove, copies: vec![] }];
        assert!(is_overridden("docs/readme.txt", &entries));
    }

    #[test]
    fn test_is_overridden_false() {
        let entries = vec![MapEntry { src_path: "docs".to_string(), primary: MapTarget::Remove, copies: vec![] }];
        assert!(!is_overridden("src/file.txt", &entries));
    }

    // -- get_empty_tree_oid --

    #[test]
    fn test_empty_tree_oid_is_valid() {
        let s = get_empty_tree_oid().unwrap().to_string();
        assert!(s.len() >= 40);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
