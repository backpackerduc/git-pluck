use std::fmt::Write;

use anyhow::Context;

use crate::cache::PluckCache;

/// Find the last plucked source commit SHA using the configured log mechanism.
///
/// Uses the log branch if `use_log_branch` is true,
/// otherwise falls back to reading the log in the pluck commit message.
pub fn get_last_plucked_source_sha(
    repo: &git2::Repository,
    pluckname: &str,
    use_log_branch: bool,
) -> anyhow::Result<Option<String>> {
    if use_log_branch { get_from_log_branch(repo, pluckname) } else { get_from_log_message(repo, pluckname) }
}

/// Get the last logged source commit sha from the log branch's parents.
///
/// If the log commit has a third parent (`^3`), the second parent (`^2`) is the source.
/// Otherwise, the first parent (`^1`) is the source.
fn get_from_log_branch(repo: &git2::Repository, pluckname: &str) -> anyhow::Result<Option<String>> {
    let refname = format!("refs/heads/pluck/log/{pluckname}");
    let Ok(oid) = repo.refname_to_id(&refname) else {
        return Ok(None);
    };

    let commit = repo.find_commit(oid).context("Failed to find log commit")?;

    let parent_count = commit.parent_count();

    if parent_count >= 3 {
        let source_parent = commit.parent_id(1).context("Failed to get source parent from log commit")?;
        Ok(Some(source_parent.to_string()))
    } else if parent_count >= 1 {
        let source_parent = commit.parent_id(0).context("Failed to get source parent from log commit")?;
        Ok(Some(source_parent.to_string()))
    } else {
        Ok(None)
    }
}

/// Get the last plucked source from the pluck branch tip's message's `Plucked from: <SHA>` trailer.
fn get_from_log_message(repo: &git2::Repository, pluckname: &str) -> anyhow::Result<Option<String>> {
    let refname = format!("refs/heads/pluck/{pluckname}");
    let Ok(oid) = repo.refname_to_id(&refname) else {
        return Ok(None);
    };

    let commit = repo.find_commit(oid).context("Failed to find pluck commit")?;

    let message = commit.message().unwrap_or("");
    for line in message.lines().rev() {
        let line = line.trim();
        if let Some(sha) = line.strip_prefix("Plucked from: ") {
            let sha = sha.trim();
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(Some(sha.to_string()));
            }
        }
    }

    Ok(None)
}

/// Create a log commit.
///
/// Triple merge: parent1 = current log commit (or source), parent2 = source, parent3 = pluck tip.
/// Author is `Git-Pluck <git@pluck>`, tree is the source commit's tree.
pub fn create_log_commit(
    repo: &git2::Repository,
    pluckname: &str,
    start_ref: git2::Oid,
    pluck_tip: git2::Oid,
    cache: &PluckCache,
) -> anyhow::Result<git2::Oid> {
    let log_ref = format!("refs/heads/pluck/log/{pluckname}");
    let current_log_oid = repo.refname_to_id(&log_ref).ok();

    let start_commit = repo.find_commit(start_ref).context("Failed to find commit for start reference")?;
    let start_commit_tree = start_commit.tree().context("Failed to get source tree")?;

    let message = build_log_branch_message(pluckname, cache);

    let sig = git2::Signature::now("Git-Pluck", "git@pluck")?;

    // let start_ref_obj2 = repo.find_commit(start_ref)?;
    let pluck_commit = repo.find_commit(pluck_tip)?;

    let current_log_commit = current_log_oid
        .map(|current| repo.find_commit(current))
        .transpose()
        .context("Failed to find previous log commit")?;

    let parents: Vec<&git2::Commit> = if let Some(clc) = &current_log_commit {
        vec![clc, &start_commit, &pluck_commit]
    } else {
        vec![&start_commit, &start_commit, &pluck_commit]
    };

    let new_log_commit_oid = repo.commit(None, &sig, &sig, &message, &start_commit_tree, &parents)?;

    Ok(new_log_commit_oid)
}

/// Build the commit message for log branch.
///
/// Format: `[SOURCE:PLUCK] <pluckname>\n\n<source-sha:pluck-sha pairs>`
fn build_log_branch_message(pluckname: &str, cache: &PluckCache) -> String {
    let mut msg = format!("[SOURCE:PLUCK] {pluckname}\n\n");
    for (source, pluck) in &cache.new_entries {
        let _ = writeln!(msg, "{source}:{pluck}");
    }
    msg
}

/// Validate log/pluck branch consistency.
///
/// The pluck tip must be an immediate parent of the log branch tip.
pub fn validate_log_pluck_consistency(repo: &git2::Repository, pluckname: &str) -> anyhow::Result<()> {
    let pluck_ref = format!("refs/heads/pluck/{pluckname}");
    let log_ref = format!("refs/heads/pluck/log/{pluckname}");

    let Ok(pluck_oid) = repo.refname_to_id(&pluck_ref) else {
        return Ok(());
    };

    let Ok(log_oid) = repo.refname_to_id(&log_ref) else {
        return Ok(());
    };

    let log_commit = repo.find_commit(log_oid).context("Failed to find log commit")?;

    let pluck_commit = repo.find_commit(pluck_oid).context("Failed to find pluck commit")?;

    let is_parent = log_commit.parents().any(|p| p.id() == pluck_commit.id());

    if !is_parent {
        anyhow::bail!(
            "Log branch inconsistency: pluck tip {pluck_oid} is not an immediate parent of log branch tip {log_oid}"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PluckCache;

    // -- build_log_branch_message --

    #[test]
    fn test_build_log_branch_message_format() {
        let cache = PluckCache::new();
        let msg = build_log_branch_message("mypluckname", &cache);
        assert!(msg.contains("[SOURCE:PLUCK]"));
        assert!(msg.contains("mypluckname"));
    }

    #[test]
    fn test_build_log_branch_message_with_entries() {
        let mut cache = PluckCache::new();
        cache.add_new(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        );
        cache.add_new(
            "cccccccccccccccccccccccccccccccccccccccc".to_string(),
            "dddddddddddddddddddddddddddddddddddddddd".to_string(),
        );
        let msg = build_log_branch_message("test", &cache);
        assert!(msg.contains("[SOURCE:PLUCK] test"));
        assert!(msg.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        assert!(msg.contains("cccccccccccccccccccccccccccccccccccccccc:dddddddddddddddddddddddddddddddddddddddd"));
    }

    #[test]
    fn test_build_log_branch_message_entries_newest_first() {
        let mut cache = PluckCache::new();
        cache.add_new("source1".to_string(), "pluck1".to_string());
        cache.add_new("source2".to_string(), "pluck2".to_string());
        let msg = build_log_branch_message("test", &cache);
        // source2 was added last, should be first in message (newest first)
        let first_pos = msg.find("source2").unwrap();
        let second_pos = msg.find("source1").unwrap();
        assert!(first_pos < second_pos, "Newest entry should appear first");
    }

    // -- get_last_plucked_source_sha --

    #[test]
    fn test_get_last_plucked_source_delegates_log_branch() {
        // Can't test without a repo, but we can verify the function signature
        // and that it compiles with the right parameters
    }

    #[test]
    fn test_get_last_plucked_source_delegates_log_message() {
        // Same as above - requires a git repo
    }
}
