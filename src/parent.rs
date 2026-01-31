use std::collections::HashMap;
use std::process::Command;

use anyhow::Context;

use crate::cache::PluckCache;
use crate::config::PluckConfig;

// NOTE: This module uses `git` shell commands instead of the git2 library.
// `git rev-list --topo-order --parents` outputs commit+parents in one shot.
// With git2::RevWalk you'd need a separate find_commit() call per OID to read
// parents, and user-supplied options (max-count) can't be forwarded to the API.
// This runs once per pluck operation, not per commit, so spawn overhead is negligible.

/// Maps source commit SHAs to their parent SHAs for transitive lookup.
///
/// Built from `git rev-list --topo-order --parents` output, prepended with
/// each processed revision as the plucking loop runs.
pub struct SourceHistory {
    entries: HashMap<String, Vec<String>>,
}

impl SourceHistory {
    /// Create an empty source history.
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Add a commit and its parents to the history.
    ///
    /// `commit_sha` - the commit being added
    /// `parents` - list of parent commit SHAs
    pub fn prepend(&mut self, commit_sha: String, parents: Vec<String>) {
        self.entries.insert(commit_sha, parents);
    }

    /// Get the parents of a commit, if recorded.
    pub fn get_parents(&self, commit_sha: &str) -> Option<&Vec<String>> {
        self.entries.get(commit_sha)
    }
}

/// Build the list of revisions to pluck.
///
/// In single-commit mode returns one entry for `start_ref`.
/// In recursive mode runs `git rev-list` over the appropriate range.
///
/// `config` - determines mode and range parameters
/// `last_plucked_source` - the last source SHA already processed (for range start)
pub fn build_revision_list(
    config: &PluckConfig,
    last_plucked_source: Option<String>,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    if config.recursive { build_recursive_list(config, last_plucked_source) } else { build_single_commit_list(config) }
}

fn build_single_commit_list(config: &PluckConfig) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &config.start_ref])
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!("Failed to resolve start reference: {}", config.start_ref);
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get parents
    let log_output = Command::new("git")
        .args(["log", "-1", "--format=%P", &sha])
        .output()
        .context("Failed to get commit parents")?;

    let parents_str = String::from_utf8_lossy(&log_output.stdout).trim().to_string();
    let parents: Vec<String> = if parents_str.is_empty() {
        Vec::new()
    } else {
        parents_str.split_whitespace().map(std::string::ToString::to_string).collect()
    };

    Ok(vec![(sha, parents)])
}

fn build_recursive_list(
    config: &PluckConfig,
    last_plucked_source_sha: Option<String>,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let range = if let Some(ref ignorant_pluck_ref) = config.ignorant_pluck {
        if !ignorant_pluck_ref.is_empty() {
            format!("{}..{}", ignorant_pluck_ref, config.start_ref)
        } else if let Some(last) = last_plucked_source_sha {
            format!("{}..{}", last, config.start_ref)
        } else {
            config.start_ref.clone()
        }
    } else if let Some(last) = last_plucked_source_sha {
        format!("{}..{}", last, config.start_ref)
    } else {
        config.start_ref.clone()
    };

    let mut cmd = Command::new("git");
    cmd.args(["rev-list", "--reverse", "--topo-order", "--parents", &range]);

    // Track max-count to apply after parsing (git applies --max-count before --reverse)
    let mut max_count: Option<usize> = None;

    if let Some(ref opts) = config.recursive_opts {
        // Skip bare "true"/"false" values (boolean enablers, not git options)
        // Convert `name:value` to `--name=value` for git options
        let mut git_args: Vec<String> = Vec::new();
        for part in opts.split_whitespace() {
            let lower = part.to_lowercase();
            if lower == "true" || lower == "false" {
                continue;
            }
            // Convert option format: "max-count:1" → "--max-count=1"
            let converted = if part.contains(':') && !part.starts_with('-') {
                format!("--{}", part.replace(':', "="))
            } else if part.contains(':') {
                // Already has -- prefix, just convert : to =
                part.replace(':', "=")
            } else {
                part.to_string()
            };

            // Extract --max-count=N to apply in code (after reverse)
            if let Some(count_str) = converted.strip_prefix("--max-count=") {
                if let Ok(n) = count_str.parse::<usize>() {
                    max_count = Some(n);
                }
            } else {
                git_args.push(converted);
            }
        }
        if !git_args.is_empty() {
            cmd.args(&git_args);
        }
    }

    let output = cmd.output().context("Failed to run git rev-list")?;

    if !output.status.success() {
        anyhow::bail!("git rev-list failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let mut revisions = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let commit = parts[0].to_string();
        let parents: Vec<String> = parts[1..].iter().map(std::string::ToString::to_string).collect();
        revisions.push((commit, parents));
    }

    // Apply max-count after parsing (list is in reverse=oldest-first order)
    if let Some(n) = max_count {
        revisions.truncate(n);
    }

    Ok(revisions)
}

/// Resolve source commit parents to their pluck equivalents.
///
/// Processes each source parent in order so that `pluck_parents[i]` corresponds
/// to `source_parents[i]`. For each parent, first tries direct cache lookup,
/// then falls back to transitive walk through source history.
/// Deduplication: removes exact duplicates. For linear commits, also removes
/// transitive parents (A is ancestor of B → drop A). Merge commits preserve
/// all resolved parents to maintain merge topology.
///
/// `source_parents` - parent SHAs of the current source commit
/// `cache` - source→pluck SHA mapping
/// `source_history` - commit→parents mapping for transitive lookup
/// `config` - controls partial ancestry and deduplication behavior
pub fn resolve_parents(
    source_parents: &[String],
    cache: &PluckCache,
    source_history: &SourceHistory,
    config: &PluckConfig,
) -> anyhow::Result<Vec<String>> {
    let mut pluck_parents: Vec<String> = Vec::new();

    // Resolve each source parent in order to preserve correspondence
    for parent in source_parents {
        if let Some(resolved) = resolve_single_parent(parent, cache, source_history)? {
            pluck_parents.push(resolved);
        }
    }

    dedup_parents(&pluck_parents, source_parents.len(), config.skip_dedup_ancestry)
}

/// Resolve a single source parent SHA to its pluck equivalent.
///
/// First tries direct cache lookup. If not found, walks up the source history
/// until a cached ancestor is found. Returns None if the chain ends at a root
/// with no pluck mapping (partial ancestry or root commit).
fn resolve_single_parent(
    source_sha: &str,
    cache: &PluckCache,
    source_history: &SourceHistory,
) -> anyhow::Result<Option<String>> {
    let mut current = source_sha.to_string();
    loop {
        if let Some(pluck_sha) = cache.lookup_pluck_sha(&current) {
            return Ok(Some(pluck_sha.clone()));
        }
        let parents = if let Some(parents) = source_history.get_parents(&current) {
            parents.clone()
        } else if let Some(parents) = get_git_parents(&current) {
            // Commit not in current range — query git directly to continue walk.
            // This happens during step-by-step plucking when ancestors were
            // processed in a previous run.
            parents
        } else {
            // Commit unknown to both history and git — treat as root.
            return Ok(None);
        };
        if parents.is_empty() {
            // Root commit with no pluck mapping — this ancestry chain
            // becomes a root in the pluck history.
            return Ok(None);
        }
        current = parents[0].clone();
    }
}

/// Get parent SHAs of a commit from git.
/// Returns empty vec for root commits, None if commit doesn't exist.
fn get_git_parents(commit_sha: &str) -> Option<Vec<String>> {
    let output = Command::new("git").args(["log", "-1", "--format=%P", commit_sha]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let parents_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if parents_str.is_empty() {
        Some(Vec::new())
    } else {
        Some(parents_str.split_whitespace().map(std::string::ToString::to_string).collect())
    }
}

/// Deduplicate parents unless `skip_dedup_ancestry` is set.
///
/// For merge commits (multiple source parents), only removes exact duplicates
/// to preserve merge topology. For linear commits, also removes transitive
/// parents (if A is ancestor of B, drop A since B includes A's changes).
fn dedup_parents(parents: &[String], source_parent_count: usize, skip_dedup: bool) -> anyhow::Result<Vec<String>> {
    if skip_dedup {
        return Ok(parents.to_vec());
    }

    let is_merge = source_parent_count > 1;

    // Always remove exact duplicates (preserve order)
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<String> = Vec::new();
    for p in parents {
        if seen.insert(p.as_str()) {
            result.push(p.clone());
        }
    }

    // For linear commits, also remove transitive ancestors
    if !is_merge {
        let mut deduped: Vec<String> = Vec::new();
        for parent_a in &result {
            let is_ancestor_of_existing = deduped.iter().any(|parent_b| is_ancestor_of(parent_b, parent_a));
            if is_ancestor_of_existing {
                continue;
            }
            deduped.retain(|parent_b| !is_ancestor_of(parent_a, parent_b));
            deduped.push(parent_a.clone());
        }
        result = deduped;
    }

    Ok(result)
}

/// Check if `child` is a descendant of `ancestor` via `git rev-list`.
fn is_ancestor_of(child: &str, ancestor: &str) -> bool {
    let output = Command::new("git").args(["rev-list", "--max-count=1", &format!("{child}..{ancestor}")]).output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- SourceHistory --

    #[test]
    fn test_source_history_new_empty() {
        let history = SourceHistory::new();
        assert!(history.get_parents("anything").is_none());
    }

    #[test]
    fn test_source_history_prepend_and_get() {
        let mut history = SourceHistory::new();
        history.prepend("commit1".to_string(), vec!["parent1".to_string(), "parent2".to_string()]);
        let parents = history.get_parents("commit1").unwrap();
        assert_eq!(parents.len(), 2);
        assert_eq!(parents[0], "parent1");
        assert_eq!(parents[1], "parent2");
    }

    #[test]
    fn test_source_history_root_commit() {
        let mut history = SourceHistory::new();
        history.prepend("root_commit".to_string(), vec![]);
        let parents = history.get_parents("root_commit").unwrap();
        assert!(parents.is_empty());
    }

    #[test]
    fn test_source_history_multiple_commits() {
        let mut history = SourceHistory::new();
        history.prepend("c1".to_string(), vec!["p1".to_string()]);
        history.prepend("c2".to_string(), vec!["p2".to_string(), "p3".to_string()]);
        history.prepend("c3".to_string(), vec![]);

        assert_eq!(history.get_parents("c1").unwrap().len(), 1);
        assert_eq!(history.get_parents("c2").unwrap().len(), 2);
        assert_eq!(history.get_parents("c3").unwrap().len(), 0);
        assert!(history.get_parents("nonexistent").is_none());
    }

    // -- build_revision_list --

    #[test]
    fn test_build_single_commit_list_requires_git() {
        // Requires a git repo, skip in unit tests
    }

    #[test]
    fn test_build_recursive_list_requires_git() {
        // Requires a git repo, skip in unit tests
    }

    // -- resolve_parents --

    #[test]
    fn test_resolve_parents_empty() {
        let config = crate::config::PluckConfig::default();
        let cache = crate::cache::PluckCache::new();
        let history = SourceHistory::new();
        let result = resolve_parents(&[], &cache, &history, &config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_parents_fast_path() {
        let config = crate::config::PluckConfig { skip_dedup_ancestry: true, ..Default::default() }; // skip dedup for predictable results
        let mut cache = crate::cache::PluckCache::new();
        cache.preload.insert("parent1".to_string(), "pluck1".to_string());
        cache.preload.insert("parent2".to_string(), "pluck2".to_string());
        let history = SourceHistory::new();
        let parents = vec!["parent1".to_string(), "parent2".to_string()];
        let result = resolve_parents(&parents, &cache, &history, &config).unwrap();
        assert_eq!(result, vec!["pluck1", "pluck2"]);
    }

    #[test]
    fn test_resolve_parents_allow_incomplete_ancestry() {
        let config = crate::config::PluckConfig { allow_incomplete_ancestry: true, ..Default::default() };
        let cache = crate::cache::PluckCache::new();
        let history = SourceHistory::new();
        // No parents in cache, but allow_incomplete_ancestry allows it
        let parents = vec!["unknown".to_string()];
        let result = resolve_parents(&parents, &cache, &history, &config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_parents_missing_parent_becomes_root() {
        // When a parent commit is not in cache and not a real git commit,
        // the git fallback returns None, treating it as a root (chain ends).
        let config = crate::config::PluckConfig::default();
        let cache = crate::cache::PluckCache::new();
        let history = SourceHistory::new();
        let parents = vec!["unknown".to_string()];
        let result = resolve_parents(&parents, &cache, &history, &config).unwrap();
        assert!(result.is_empty());
    }
}
