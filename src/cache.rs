use std::collections::HashMap;

use anyhow::Context;

use crate::config::PluckConfig;

/// Cache of source-sha -> pluck-sha mappings.
///
/// `preload` is a HashMap lookup built from log data.
/// `new_entries` accumulates newly created pairs during the current run (newest first).
pub struct PluckCache {
    pub preload: HashMap<String, String>,
    pub new_entries: Vec<(String, String)>,
}

impl PluckCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self { preload: HashMap::new(), new_entries: Vec::new() }
    }

    /// Preload the cache from existing log data.
    ///
    /// Reads log branch commits or pluck commit message depending on config.
    /// `repo` - the git repository
    /// `pluckname` - the name for ref path resolution
    /// `config` - determines which log mechanism to use
    pub fn preload_from_log(
        &mut self,
        repo: &git2::Repository,
        pluckname: &str,
        config: &PluckConfig,
    ) -> anyhow::Result<()> {
        if config.log_branch {
            self.load_from_log_branch(repo, pluckname)?;
        }
        if config.log_message {
            self.load_from_log_message(repo, pluckname, config)?;
        }
        Ok(())
    }

    /// Load source→pluck pairs from the log branch commit messages.
    fn load_from_log_branch(&mut self, repo: &git2::Repository, pluckname: &str) -> anyhow::Result<()> {
        let refname = format!("refs/heads/pluck/log/{pluckname}");
        let Ok(ref_obj) = repo.refname_to_id(&refname) else {
            return Ok(());
        };

        let commit = repo.find_commit(ref_obj).context("Failed to find log commit")?;

        let mut rw = repo.revwalk()?;
        rw.push(commit.id())?;
        rw.set_sorting(git2::Sort::TOPOLOGICAL)?;

        for oid in rw {
            let oid = oid?;
            let commit = repo.find_commit(oid).context("Failed to find commit in log branch")?;
            let message = commit.message().unwrap_or("");

            for line in message.lines() {
                let line = line.trim();
                if is_sha_pair(line) {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() == 2 {
                        self.preload.insert(parts[0].to_string(), parts[1].to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Load source-pluck pairs from `Plucked from: <SHA>` on existing pluck commits messages.
    fn load_from_log_message(
        &mut self,
        repo: &git2::Repository,
        pluckname: &str,
        config: &PluckConfig,
    ) -> anyhow::Result<()> {
        let refname = format!("refs/heads/pluck/{pluckname}");
        let Ok(ref_obj) = repo.refname_to_id(&refname) else {
            return Ok(());
        };

        let commit = repo.find_commit(ref_obj).context("Failed to find pluck commit")?;

        let mut rw = repo.revwalk()?;
        rw.push(commit.id())?;
        rw.set_sorting(git2::Sort::TOPOLOGICAL)?;

        for oid in rw {
            let oid = oid?;
            let commit = repo.find_commit(oid).context("Failed to find commit")?;
            let message = commit.message().unwrap_or("");

            if let Some(source_sha) = extract_pluck_source_sha(message) {
                self.preload.insert(source_sha, commit.id().to_string());
            } else if !config.force {
                anyhow::bail!("Commit {} is missing Pluck source SHA information", commit.id());
            }
        }

        Ok(())
    }

    /// Look up the pluck SHA for a given source SHA.
    pub fn lookup_pluck_sha(&self, source_sha: &str) -> Option<&String> {
        self.preload.get(source_sha)
    }

    /// Look up the source SHA for a given pluck SHA (reverse lookup).
    pub fn lookup_source_sha(&self, pluck_sha: &str) -> Option<&String> {
        for (source, pluck) in &self.preload {
            if pluck == pluck_sha {
                return Some(source);
            }
        }
        None
    }

    /// Add a new source-pluck pair, prepending to `new_entries` (newest first).
    pub fn add_new(&mut self, source_sha: String, pluck_sha: String) {
        self.preload.insert(source_sha.clone(), pluck_sha.clone());
        self.new_entries.insert(0, (source_sha, pluck_sha));
    }
}

/// Check if a line is a valid `source-sha:pluck-sha` pair (two 40-char hex strings).
fn is_sha_pair(line: &str) -> bool {
    let parts: Vec<&str> = line.split(':').collect();
    parts.len() == 2
        && parts[0].len() == 40
        && parts[1].len() == 40
        && parts[0].chars().all(|c| c.is_ascii_hexdigit())
        && parts[1].chars().all(|c| c.is_ascii_hexdigit())
}

/// Extract the source SHA from a pluck commit message.
fn extract_pluck_source_sha(message: &str) -> Option<String> {
    for line in message.lines().rev() {
        let line = line.trim();
        if let Some(sha) = line.strip_prefix("Plucked from: ") {
            let sha = sha.trim();
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(sha.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sha_pair_valid() {
        assert!(is_sha_pair("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }

    #[test]
    fn test_is_sha_pair_lowercase_hex() {
        assert!(is_sha_pair("abcdef0123456789abcdef0123456789abcdef01:abcdef0123456789abcdef0123456789abcdef01"));
    }

    #[test]
    fn test_is_sha_pair_uppercase_hex() {
        assert!(is_sha_pair("ABCDEF0123456789ABCDEF0123456789ABCDEF01:ABCDEF0123456789ABCDEF0123456789ABCDEF01"));
    }

    #[test]
    fn test_is_sha_pair_too_short() {
        assert!(!is_sha_pair("aaaaa:bbbbb"));
    }

    #[test]
    fn test_is_sha_pair_too_long() {
        assert!(!is_sha_pair("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }

    #[test]
    fn test_is_sha_pair_non_hex() {
        assert!(!is_sha_pair("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }

    #[test]
    fn test_is_sha_pair_missing_colon() {
        assert!(!is_sha_pair("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }

    #[test]
    fn test_is_sha_pair_extra_colon() {
        assert!(!is_sha_pair("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:bbbb:cccc"));
    }

    #[test]
    fn test_is_sha_pair_empty() {
        assert!(!is_sha_pair(""));
    }

    #[test]
    fn test_is_sha_pair_empty_parts() {
        assert!(!is_sha_pair(":"));
        assert!(!is_sha_pair("aaaaaa:"));
        assert!(!is_sha_pair(":bbbbbb"));
    }

    #[test]
    fn test_extract_pluck_source_sha_valid() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let msg = format!("Some commit message\n\nPlucked from: {}", sha);
        assert_eq!(extract_pluck_source_sha(&msg), Some(sha.to_string()));
    }

    #[test]
    fn test_extract_pluck_source_sha_only_trailer() {
        let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let msg = format!("Plucked from: {}", sha);
        assert_eq!(extract_pluck_source_sha(&msg), Some(sha.to_string()));
    }

    #[test]
    fn test_extract_pluck_source_sha_with_other_trailers() {
        let sha = "cccccccccccccccccccccccccccccccccccccccc";
        let msg = format!("Commit message\n\nSome-Other: value\nPlucked from: {}\nAnother: stuff", sha);
        assert_eq!(extract_pluck_source_sha(&msg), Some(sha.to_string()));
    }

    #[test]
    fn test_extract_pluck_source_sha_missing() {
        let msg = "Just a regular commit message\n\nNo trailers here";
        assert!(extract_pluck_source_sha(msg).is_none());
    }

    #[test]
    fn test_extract_pluck_source_sha_invalid_sha() {
        let msg = "Plucked from: not-a-valid-sha";
        assert!(extract_pluck_source_sha(msg).is_none());
    }

    #[test]
    fn test_extract_pluck_source_sha_too_short() {
        let msg = "Plucked from: aaaa";
        assert!(extract_pluck_source_sha(msg).is_none());
    }

    #[test]
    fn test_extract_pluck_source_sha_last_wins() {
        // Should find the last Plucked from SHA trailer (searches from bottom)
        let sha1 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let sha2 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let msg = format!("Message\n\nPlucked from: {sha1}\nPlucked from: {sha2}");
        assert_eq!(extract_pluck_source_sha(&msg), Some(sha2.to_string()));
    }

    #[test]
    fn test_pluck_cache_new() {
        let cache = PluckCache::new();
        assert!(cache.preload.is_empty());
        assert!(cache.new_entries.is_empty());
    }

    #[test]
    fn test_pluck_cache_add_new_prepend() {
        let mut cache = PluckCache::new();
        cache.add_new("source1".to_string(), "pluck1".to_string());
        cache.add_new("source2".to_string(), "pluck2".to_string());

        // new_entries should be newest first (prepended)
        assert_eq!(cache.new_entries.len(), 2);
        assert_eq!(cache.new_entries[0], ("source2".to_string(), "pluck2".to_string()));
        assert_eq!(cache.new_entries[1], ("source1".to_string(), "pluck1".to_string()));

        // preload should have both
        assert_eq!(cache.preload.get("source1"), Some(&"pluck1".to_string()));
        assert_eq!(cache.preload.get("source2"), Some(&"pluck2".to_string()));
    }

    #[test]
    fn test_pluck_cache_lookup_pluck_sha() {
        let mut cache = PluckCache::new();
        cache.preload.insert("source1".to_string(), "pluck1".to_string());
        cache.preload.insert("source2".to_string(), "pluck2".to_string());

        assert_eq!(cache.lookup_pluck_sha("source1"), Some(&"pluck1".to_string()));
        assert_eq!(cache.lookup_pluck_sha("source2"), Some(&"pluck2".to_string()));
        assert!(cache.lookup_pluck_sha("nonexistent").is_none());
    }

    #[test]
    fn test_pluck_cache_lookup_source_sha() {
        let mut cache = PluckCache::new();
        cache.preload.insert("source1".to_string(), "pluck1".to_string());
        cache.preload.insert("source2".to_string(), "pluck2".to_string());

        assert_eq!(cache.lookup_source_sha("pluck1"), Some(&"source1".to_string()));
        assert_eq!(cache.lookup_source_sha("pluck2"), Some(&"source2".to_string()));
        assert!(cache.lookup_source_sha("nonexistent").is_none());
    }

    #[test]
    fn test_pluck_cache_lookup_source_sha_empty() {
        let cache = PluckCache::new();
        assert!(cache.lookup_source_sha("anything").is_none());
    }
}
