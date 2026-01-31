use std::io::Write;
use std::process::Command;

use anyhow::Context;
use regex::Regex;

use crate::cache::PluckCache;
use crate::config::PluckConfig;
use crate::log::{create_log_commit, get_last_plucked_source_sha, validate_log_pluck_consistency};
use crate::map_modify::build_pluck_map;
use crate::parent::{SourceHistory, build_revision_list, resolve_parents};
use crate::tree::{build_pluck_tree, create_tree_objects, get_empty_tree_oid};

/// Run the full plucking pipeline.
///
/// Validates config, builds the revision list, preloads the cache,
/// then processes each revision: build tree, create objects, resolve parents,
/// create commits, and update refs.
///
/// `repo` - the git repository
/// `pluckname` - the Pluck name for ref paths
/// `config` - merged config from file + CLI
pub fn run_pluck(
    repo: &git2::Repository,
    pluckname: &str,
    config: &PluckConfig,
) -> anyhow::Result<()> {
    validate_sanity_checks(config)?;

    let map_entries = build_pluck_map(config)?;

    validate_log_pluck_consistency(repo, pluckname)?;

    let last_source = get_last_plucked_source_sha(repo, pluckname, config.log_branch)?;

    let revisions = build_revision_list(config, last_source)?;

    if revisions.is_empty() && !config.force && !config.allow_unchanged_tree {
        anyhow::bail!("No revisions to pluck");
    }

    let mut cache = PluckCache::new();
    cache.preload_from_log(repo, pluckname, config)?;

    let mut source_history = SourceHistory::new();
    let mut created_commits = 0;
    let mut last_pluck_sha = None;

    let total = revisions.len();

    for (idx, (commit_sha, parents)) in revisions.iter().enumerate() {
        source_history.prepend(commit_sha.clone(), parents.clone());

        let pluck_tree = build_pluck_tree(commit_sha, &map_entries, config)?;

        let tree_oid = create_tree_objects(&pluck_tree)?;

        if is_empty_tree(tree_oid) {
            print_progress(idx + 1, total, created_commits, config);
            continue;
        }

        let pluck_parents = if config.recursive {
            resolve_parents(parents, &cache, &source_history, config)?
        } else {
            resolve_single_commit_parents(repo, pluckname, config)?
        };

        let skip = check_tree_unchanged(repo, &pluck_parents, tree_oid, config)?;

        if skip {
            print_progress(idx + 1, total, created_commits, config);
            continue;
        }

        let commit_oid = create_pluck_commit(repo, commit_sha, tree_oid, &pluck_parents, config)?;

        cache.add_new(commit_sha.clone(), commit_oid.to_string());
        created_commits += 1;
        last_pluck_sha = Some(commit_oid);

        update_progress_ref(repo, pluckname, commit_oid)?;

        print_progress(idx + 1, total, created_commits, config);
    }

    final_ref_updates(repo, pluckname, config, &cache, total, created_commits, last_pluck_sha)?;

    if created_commits == 0 && !config.force && !config.allow_missing_path {
        anyhow::bail!("No pluck commits created");
    }

    Ok(())
}

/// Run pre-flight sanity checks.
fn validate_sanity_checks(config: &PluckConfig) -> anyhow::Result<()> {
    if config.allow_unchanged_tree && config.recursive {
        anyhow::bail!("--allow-unchanged-tree and --recursive cannot be combined");
    }

    if !config.log_branch && !config.log_message {
        anyhow::bail!("At least one of --log-branch or --log-message must be enabled");
    }

    if config.rep_author_regex.is_some() && (config.rep_author_name.is_none() || config.rep_author_email.is_none()) {
        anyhow::bail!("--rep-author-regex requires --rep-author-name and --rep-author-email");
    }

    if config.rep_committer_regex.is_some()
        && (config.rep_committer_name.is_none() || config.rep_committer_email.is_none())
    {
        anyhow::bail!("--rep-committer-regex requires --rep-committer-name and --rep-committer-email");
    }

    Ok(())
}

/// Resolve parent for single-commit mode.
///
/// Uses `--ignorant-pluck` if set, otherwise the current pluck branch tip.
fn resolve_single_commit_parents(
    repo: &git2::Repository,
    pluckname: &str,
    config: &PluckConfig,
) -> anyhow::Result<Vec<String>> {
    if let Some(ref ignorant_pluck_ref) = config.ignorant_pluck {
        if !ignorant_pluck_ref.is_empty()
            && let Ok(oid) = repo.refname_to_id(ignorant_pluck_ref)
        {
            return Ok(vec![oid.to_string()]);
        }
    } else {
        let pluck_ref = format!("refs/heads/pluck/{pluckname}");
        if let Ok(oid) = repo.refname_to_id(&pluck_ref) {
            return Ok(vec![oid.to_string()]);
        }
    }

    Ok(Vec::new())
}

/// Check if the new tree is unchanged from the first parent's tree.
fn check_tree_unchanged(
    repo: &git2::Repository,
    pluck_parents: &[String],
    tree_oid: git2::Oid,
    config: &PluckConfig,
) -> anyhow::Result<bool> {
    if pluck_parents.is_empty() || config.allow_unchanged_tree {
        return Ok(false);
    }

    let parent_oid = git2::Oid::from_str(&pluck_parents[0]).context("Invalid parent SHA")?;
    let first_parent = repo.find_commit(parent_oid).ok().and_then(|c| c.tree().ok());

    Ok(first_parent.is_some_and(|parent_tree| parent_tree.id() == tree_oid))
}

/// Create a pluck commit from an source commit metadata.
///
/// Preserves dates, optionally replaces author/committer/message,
/// and adds source sha trailer if enabled.
///
/// `source_sha` - the source commit SHA to base metadata from
/// `tree_oid` - the pluck tree OID
/// `parents` - resolved pluck parent commit SHAs
fn create_pluck_commit(
    repo: &git2::Repository,
    source_sha: &str,
    tree_oid: git2::Oid,
    parents: &[String],
    config: &PluckConfig,
) -> anyhow::Result<git2::Oid> {
    let source_oid = git2::Oid::from_str(source_sha).context("Invalid source SHA")?;
    let start_ref = repo.find_commit(source_oid).context("Failed to find source commit")?;

    let (author_name, author_email) = resolve_author(config, &start_ref);
    let (committer_name, committer_email) = resolve_committer(config, &start_ref);

    let author_sig = git2::Signature::new(
        &author_name,
        &author_email,
        &git2::Time::new(start_ref.author().when().seconds(), start_ref.author().when().offset_minutes() * 60),
    )?;

    let committer_sig = git2::Signature::new(
        &committer_name,
        &committer_email,
        &git2::Time::new(start_ref.committer().when().seconds(), start_ref.committer().when().offset_minutes() * 60),
    )?;

    let message = resolve_message(config, start_ref.message().unwrap_or(""))?;
    let message = if config.log_message { add_source_sha(&message, source_sha)? } else { message };

    let tree = repo.find_tree(tree_oid).context("Failed to find tree")?;

    let parent_commits: Vec<git2::Commit> = parents
        .iter()
        .map(|sha| {
            let oid = git2::Oid::from_str(sha).with_context(|| format!("Invalid parent SHA: {sha}"))?;
            repo.find_commit(oid).with_context(|| format!("Failed to find parent commit: {sha}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();

    let commit_oid = repo.commit(None, &author_sig, &committer_sig, &message, &tree, &parent_refs)?;

    Ok(commit_oid)
}

/// Resolve author name/email with optional replacement.
///
/// If `repAuthorRegex` is set, replacement occurs when the source email
/// does NOT match the allow pattern OR matches the deny pattern.
/// Otherwise `repAuthorName`/`repAuthorEmail` replace unconditionally.
fn resolve_author(config: &PluckConfig, source: &git2::Commit) -> (String, String) {
    let mut name = source.author().name().unwrap_or("").to_string();
    let mut email = source.author().email().unwrap_or("").to_string();

    if let Some(ref regex_str) = config.rep_author_regex {
        if should_replace_email(regex_str, &email) {
            if let Some(ref replacement) = config.rep_author_name
                && !replacement.is_empty()
            {
                name.clone_from(replacement);
            }
            if let Some(ref replacement) = config.rep_author_email
                && !replacement.is_empty()
            {
                email.clone_from(replacement);
            }
        }
    } else {
        if let Some(ref replacement) = config.rep_author_name
            && !replacement.is_empty()
        {
            name.clone_from(replacement);
        }
        if let Some(ref replacement) = config.rep_author_email
            && !replacement.is_empty()
        {
            email.clone_from(replacement);
        }
    }

    (name, email)
}

/// Resolve committer name/email with the same logic as `resolve_author`.
fn resolve_committer(config: &PluckConfig, source: &git2::Commit) -> (String, String) {
    let mut name = source.committer().name().unwrap_or("").to_string();
    let mut email = source.committer().email().unwrap_or("").to_string();

    if let Some(ref regex_str) = config.rep_committer_regex {
        if should_replace_email(regex_str, &email) {
            if let Some(ref replacement) = config.rep_committer_name
                && !replacement.is_empty()
            {
                name.clone_from(replacement);
            }
            if let Some(ref replacement) = config.rep_committer_email
                && !replacement.is_empty()
            {
                email.clone_from(replacement);
            }
        }
    } else {
        if let Some(ref replacement) = config.rep_committer_name
            && !replacement.is_empty()
        {
            name.clone_from(replacement);
        }
        if let Some(ref replacement) = config.rep_committer_email
            && !replacement.is_empty()
        {
            email.clone_from(replacement);
        }
    }

    (name, email)
}

/// Determine if an email should be replaced based on allow/deny regex patterns.
///
/// `regex_str` format: `allow_pattern[:deny_pattern]`. If no `:`, deny defaults to `^$`.
/// Returns true when email does NOT match allow OR matches deny.
fn should_replace_email(regex_str: &str, email: &str) -> bool {
    let parts: Vec<&str> = regex_str.splitn(2, ':').collect();
    let allow_pattern = parts[0];
    let deny_pattern = parts.get(1).unwrap_or(&"^$");

    let allow_re = Regex::new(allow_pattern).ok();
    // Only use deny pattern if explicitly provided (non-empty string and valid regex).
    // When regex_str has no colon, deny_pattern is "^$" (matches nothing).
    // When regex_str has colon but empty right side (e.g., "allow:"), skip deny.
    let deny_re = if deny_pattern.is_empty() { None } else { Regex::new(deny_pattern).ok() };

    if let Some(ref deny) = deny_re
        && deny.is_match(email)
    {
        return true;
    }

    if let Some(ref allow) = allow_re
        && !allow.is_match(email)
    {
        return true;
    }

    false
}

/// Resolve the commit message with optional replacement.
///
/// Priority: `repMessage` (full replacement) > `repMessageFilter` (line removal) > sourceal.
fn resolve_message(config: &PluckConfig, sourceal: &str) -> anyhow::Result<String> {
    if let Some(ref replacement) = config.rep_message {
        return Ok(replacement.clone());
    }

    if let Some(ref filter) = config.rep_message_filter {
        let re = Regex::new(filter).context(format!("Invalid message filter regex: {filter}"))?;
        let filtered: String = sourceal.lines().filter(|line| !re.is_match(line)).collect::<Vec<&str>>().join("\n");
        return Ok(filtered);
    }

    Ok(sourceal.to_string())
}

/// Add `Plucked from: <sha>` trailer to commit message.
fn add_source_sha(message: &str, source_sha: &str) -> anyhow::Result<String> {
    let trailer = format!("Plucked from: {source_sha}");

    let output = Command::new("git")
        .args(["interpret-trailers", "--if-exists", "addIfDifferent", "--trailer", &trailer])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn git interpret-trailers")?;

    let mut child = output;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(message.as_bytes())?;
    }
    drop(child.stdin.take());

    let output_result = child.wait_with_output().context("Failed to wait for git interpret-trailers")?;

    if output_result.status.success() {
        Ok(String::from_utf8_lossy(&output_result.stdout).to_string())
    } else {
        Ok(format!("{message}\n\nPlucked from: {source_sha}"))
    }
}

/// Check if an OID is the Git empty tree.
fn is_empty_tree(oid: git2::Oid) -> bool {
    oid == get_empty_tree_oid().unwrap()
}

/// Update the progress ref for crash recovery.
fn update_progress_ref(repo: &git2::Repository, pluckname: &str, commit_oid: git2::Oid) -> anyhow::Result<()> {
    let refname = format!("refs/heads/pluck/progress/{pluckname}");
    update_ref(repo, &refname, commit_oid.to_string())?;

    Ok(())
}

/// Update a git ref using `git update-ref` command (more reliable than git2 API for nested refs).
#[allow(clippy::needless_pass_by_value)]
fn update_ref(repo: &git2::Repository, refname: &str, sha: String) -> anyhow::Result<()> {
    let workdir = repo.workdir().unwrap_or_else(|| std::path::Path::new("."));
    let output = Command::new("git")
        .args(["update-ref", refname, &sha])
        .current_dir(workdir)
        .output()
        .context("Failed to run git update-ref")?;

    if !output.status.success() {
        anyhow::bail!("git update-ref failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

/// Print progress to stdout, overwritten in place with `\r`.
fn print_progress(processed: usize, total: usize, created: usize, config: &PluckConfig) {
    if config.quiet {
        return;
    }
    print!("\rPLUCKING {processed}/{total}[{created}]");
    let _ = std::io::stdout().flush();
}

/// Perform final ref updates after all revisions are processed.
fn final_ref_updates(
    repo: &git2::Repository,
    pluckname: &str,
    config: &PluckConfig,
    cache: &PluckCache,
    processed: usize,
    created_commits: usize,
    last_pluck_sha: Option<git2::Oid>,
) -> anyhow::Result<()> {
    if created_commits == 0 && !config.force {
        return Ok(());
    }

    let Some(pluck_sha) = last_pluck_sha else {
        delete_progress_ref(repo, pluckname);
        return Ok(());
    };

    if let Some(ref ignorant_pluck_ref) = config.ignorant_pluck {
        if !ignorant_pluck_ref.is_empty() {
            update_ref(repo, ignorant_pluck_ref, pluck_sha.to_string()).context("Failed to update ref")?;
        }
    } else {
        let pluck_ref = format!("refs/heads/pluck/{pluckname}");
        update_ref(repo, &pluck_ref, pluck_sha.to_string()).context("Failed to update pluck ref")?;

        if config.log_branch {
            let source_obj = repo
                .revparse_single(&config.start_ref)
                .context(format!("Failed to resolve source commit: {}", config.start_ref))?;
            let source_oid = match source_obj.into_commit() {
                Ok(c) => c.id(),
                Err(obj) => obj
                    .peel(git2::ObjectType::Commit)
                    .context("Source is not a commit")?
                    .into_commit()
                    .expect("peeled to commit type")
                    .id(),
            };
            let log_oid = create_log_commit(repo, pluckname, source_oid, pluck_sha, cache)?;

            let log_ref = format!("refs/heads/pluck/log/{pluckname}");
            update_ref(repo, &log_ref, log_oid.to_string()).context("Failed to update log ref")?;
        }
    }

    delete_progress_ref(repo, pluckname);

    if !config.quiet {
        println!();
        println!("Processed {processed} commits, created {created_commits} pluck commits, tip: {pluck_sha}");
    }

    Ok(())
}

/// Delete the progress ref on success.
fn delete_progress_ref(repo: &git2::Repository, pluckname: &str) {
    let refname = format!("refs/heads/pluck/progress/{pluckname}");
    if let Ok(mut ref_obj) = repo.find_reference(&refname) {
        let _ = ref_obj.delete();
    }
}

/// Clean up refs on failure.
///
/// Deletes the progress ref. If the pluck branch doesn't exist yet,
/// also deletes the log ref (partial first-run state).
pub fn cleanup_on_failure(repo: &git2::Repository, pluckname: &str) {
    delete_progress_ref(repo, pluckname);

    let pluck_ref = format!("refs/heads/pluck/{pluckname}");
    if repo.refname_to_id(&pluck_ref).is_err() {
        let log_commit_sha = format!("refs/heads/pluck/log/{pluckname}");
        if let Ok(mut ref_obj) = repo.find_reference(&log_commit_sha) {
            let _ = ref_obj.delete();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- should_replace_email --

    #[test]
    fn test_should_replace_email_match_allow_no_deny() {
        // Email matches allow pattern -> should NOT replace
        assert!(!should_replace_email(".*@example\\.com", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_no_match_allow() {
        // Email does NOT match allow pattern -> should replace
        assert!(should_replace_email(".*@other\\.com", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_deny_override() {
        // Email matches allow, but also matches deny -> should replace
        assert!(should_replace_email(".*@example\\.com:.*@example\\.com", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_deny_no_match() {
        // Email matches allow, doesn't match deny -> should NOT replace
        assert!(!should_replace_email(".*@example\\.com:.*@bad\\.com", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_default_deny() {
        // No deny pattern (no colon) -> deny defaults to ^$ (matches nothing)
        // If email matches allow -> should NOT replace
        assert!(!should_replace_email(".*@example\\.com", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_empty_allow() {
        // ^$ as allow pattern matches only empty strings.
        // "user@example.com" does NOT match ^$ -> should replace.
        // Default deny is ^$ which also doesn't match -> replaced by allow logic.
        assert!(should_replace_email("^$", "user@example.com"));
    }

    #[test]
    fn test_should_replace_email_empty_allow_empty_email() {
        // ^$ matches empty email for both allow and default deny.
        // Deny is checked first: ^$ matches "" -> true (should replace)
        assert!(should_replace_email("^$", ""));
    }

    #[test]
    fn test_should_replace_email_wildcard_allow() {
        // .* matches everything -> should NOT replace
        assert!(!should_replace_email(".*", "any@email.com"));
    }

    #[test]
    fn test_should_replace_email_specific_deny() {
        // Allow all, but deny specific
        assert!(should_replace_email(".*:bad@actor\\.com", "bad@actor.com"));
        assert!(!should_replace_email(".*:bad@actor\\.com", "good@actor.com"));
    }

    #[test]
    fn test_should_replace_email_invalid_regex_no_replace() {
        // Invalid allow regex -> None, default deny ^$ doesn't match email
        // allow is None so we skip the allow check -> return false (no replace)
        assert!(!should_replace_email("[invalid", "anything@example.com"));
    }

    #[test]
    fn test_should_replace_email_invalid_deny_valid_allow() {
        // Valid allow that matches, invalid deny -> deny is None (skipped)
        // Allow matches -> should NOT replace
        assert!(!should_replace_email(".*@example\\.com:[invalid", "user@example.com"));
    }

    // -- resolve_message --

    #[test]
    fn test_resolve_message_sourceal() {
        let config = PluckConfig::default();
        let msg = resolve_message(&config, "Sourceal message").unwrap();
        assert_eq!(msg, "Sourceal message");
    }

    #[test]
    fn test_resolve_message_replacement() {
        let config = PluckConfig { rep_message: Some("Replaced".to_string()), ..Default::default() };
        let msg = resolve_message(&config, "Sourceal message").unwrap();
        assert_eq!(msg, "Replaced");
    }

    #[test]
    fn test_resolve_message_filter() {
        let config = PluckConfig { rep_message_filter: Some("Secret".to_string()), ..Default::default() };
        let sourceal = "Line one\nSecret line\nLine three";
        let msg = resolve_message(&config, sourceal).unwrap();
        assert!(msg.contains("Line one"));
        assert!(!msg.contains("Secret line"));
        assert!(msg.contains("Line three"));
    }

    #[test]
    fn test_resolve_message_filter_multiline() {
        let config = PluckConfig { rep_message_filter: Some("^DEBUG".to_string()), ..Default::default() };
        let sourceal = "Commit message\n\nDEBUG: some info\nBody text\nDEBUG: more";
        let msg = resolve_message(&config, sourceal).unwrap();
        assert!(msg.contains("Commit message"));
        assert!(msg.contains("Body text"));
        assert!(!msg.contains("DEBUG"));
    }

    #[test]
    fn test_resolve_message_replacement_overrides_filter() {
        // repMessage takes priority over repMessageFilter
        let config = PluckConfig {
            rep_message: Some("Full replacement".to_string()),
            rep_message_filter: Some(".*".to_string()),
            ..Default::default()
        };
        let msg = resolve_message(&config, "Sourceal").unwrap();
        assert_eq!(msg, "Full replacement");
    }

    #[test]
    fn test_resolve_message_invalid_filter() {
        let config = PluckConfig { rep_message_filter: Some("[invalid".to_string()), ..Default::default() };
        let result = resolve_message(&config, "Sourceal");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_message_empty_sourceal() {
        let config = PluckConfig::default();
        let msg = resolve_message(&config, "").unwrap();
        assert_eq!(msg, "");
    }

    // -- validate_sanity_checks --

    #[test]
    fn test_sanity_checks_pass() {
        let config = PluckConfig::default();
        assert!(validate_sanity_checks(&config).is_ok());
    }

    #[test]
    fn test_sanity_checks_allow_unchanged_tree_recursive() {
        let config = PluckConfig { allow_unchanged_tree: true, recursive: true, ..Default::default() };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be combined"));
    }

    #[test]
    fn test_sanity_checks_no_log_branch_no_log_message() {
        let config = PluckConfig { log_branch: false, log_message: false, ..Default::default() };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be enabled"));
    }

    #[test]
    fn test_sanity_checks_log_branch_only_ok() {
        let config = PluckConfig { log_branch: true, log_message: false, ..Default::default() };
        assert!(validate_sanity_checks(&config).is_ok());
    }

    #[test]
    fn test_sanity_checks_log_message_only_ok() {
        let config = PluckConfig { log_branch: false, log_message: true, ..Default::default() };
        assert!(validate_sanity_checks(&config).is_ok());
    }

    #[test]
    fn test_sanity_checks_both_log_branch_and_message_ok() {
        let config = PluckConfig { log_branch: true, log_message: true, ..Default::default() };
        assert!(validate_sanity_checks(&config).is_ok());
    }

    #[test]
    fn test_sanity_checks_rep_author_regex_no_name() {
        let config = PluckConfig { rep_author_regex: Some(".*".to_string()), ..Default::default() };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires"));
    }

    #[test]
    fn test_sanity_checks_rep_author_regex_no_email() {
        let config = PluckConfig {
            rep_author_regex: Some(".*".to_string()),
            rep_author_name: Some("Name".to_string()),
            ..Default::default()
        };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires"));
    }

    #[test]
    fn test_sanity_checks_rep_author_regex_with_both_ok() {
        let config = PluckConfig {
            rep_author_regex: Some(".*".to_string()),
            rep_author_name: Some("Name".to_string()),
            rep_author_email: Some("email@example.com".to_string()),
            ..Default::default()
        };
        assert!(validate_sanity_checks(&config).is_ok());
    }

    #[test]
    fn test_sanity_checks_rep_committer_regex_no_name() {
        let config = PluckConfig { rep_committer_regex: Some(".*".to_string()), ..Default::default() };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires"));
    }

    #[test]
    fn test_sanity_checks_rep_committer_regex_no_email() {
        let config = PluckConfig {
            rep_committer_regex: Some(".*".to_string()),
            rep_committer_name: Some("Name".to_string()),
            ..Default::default()
        };
        let result = validate_sanity_checks(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires"));
    }

    #[test]
    fn test_sanity_checks_rep_committer_regex_with_both_ok() {
        let config = PluckConfig {
            rep_committer_regex: Some(".*".to_string()),
            rep_committer_name: Some("Name".to_string()),
            rep_committer_email: Some("email@example.com".to_string()),
            ..Default::default()
        };
        assert!(validate_sanity_checks(&config).is_ok());
    }

    // -- is_empty_tree --

    #[test]
    fn test_is_empty_tree_true() {
        let oid = get_empty_tree_oid().unwrap();
        assert!(is_empty_tree(oid));
    }

    #[test]
    fn test_is_empty_tree_false() {
        let oid = git2::Oid::from_str("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        assert!(!is_empty_tree(oid));
    }
}
