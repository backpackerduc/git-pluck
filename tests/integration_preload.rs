use test_utils::TestRepo;

// ============================================================================
// Log branch preload (first-parent walk) tests
// ============================================================================

#[test]
fn test_log_branch_preload_find_source_sha() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    let sha2 = repo.commit_file("src.txt", "v2", "commit 2");
    let sha3 = repo.commit_file("src.txt", "v3", "commit 3");

    let config_path = repo.create_config("preload_fs", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch"]);

    let pluck_tip = repo.get_ref("refs/heads/pluck/preload_fs").unwrap();

    // --find-source-sha does the reverse lookup: pluck SHA → source SHA
    let out =
        repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-source-sha", &pluck_tip]);
    let found = out.stdout.trim().to_string();
    assert_eq!(found, sha3, "Reverse lookup for pluck tip should return the newest source SHA");

    // Also verify the middle commit is found
    // Get the pluck SHA for sha2 via find-pluck-sha
    let out2 = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-pluck-sha", &sha2]);
    let pluck_sha2 = out2.stdout.trim().to_string();
    let out3 =
        repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-source-sha", &pluck_sha2]);
    assert_eq!(out3.stdout.trim(), sha2, "Reverse lookup should return sha2");
}

#[test]
fn test_log_branch_preload_multi_run_parent_chain() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");
    let sha3 = repo.commit_file("src.txt", "v3", "commit 3");
    let sha4 = repo.commit_file("src.txt", "v4", "commit 4");

    let config_path = repo.create_config("preload_multirun", "[forward.from \"src.txt\"]\n    to = dest.txt\n");

    // Run 1: pluck commits 1-3 recursively
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch", "-s", &sha3]);
    let pluck_tip_run1 = repo.get_ref("refs/heads/pluck/preload_multirun").unwrap();

    // Run 2: pluck commit 4 (single commit, defaults to HEAD)
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch"]);

    let pluck_tip_run2 = repo.get_ref("refs/heads/pluck/preload_multirun").unwrap();
    assert_ne!(pluck_tip_run1, pluck_tip_run2, "Run 2 should create a new pluck commit");

    // The new pluck commit's parent must be the previous pluck tip.
    // This only works if load_from_log_branch correctly preloaded the
    // sha3 → pluck_tip_run1 mapping from the log branch.
    let parents = repo.commit_parents(&pluck_tip_run2);
    assert_eq!(
        parents,
        vec![pluck_tip_run1.clone()],
        "Run 2 pluck commit parent should be run 1 pluck tip (preload must have recovered the mapping)"
    );

    // Verify the source chain is intact: run 2 pluck commit maps back to sha4
    let out =
        repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-source-sha", &pluck_tip_run2]);
    assert_eq!(out.stdout.trim(), sha4);

    // Verify the preload recovered a run-1 mapping: sha3's pluck SHA should be pluck_tip_run1
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-pluck-sha", &sha3]);
    assert_eq!(out.stdout.trim(), pluck_tip_run1, "find-pluck-sha for sha3 should return run 1's pluck tip");
}

#[test]
fn test_log_branch_preload_many_runs() {
    let repo = TestRepo::new();
    for i in 1..=6 {
        repo.commit_file("src.txt", &format!("v{}", i), &format!("commit {}", i));
    }

    let config_path = repo.create_config("preload_many", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Run pluck 6 times, one commit at a time
    for i in 1..=6 {
        let sha = repo.run_cmd("rev-parse", &[&format!("HEAD~{}", 6 - i)]);
        repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "-s", &sha]);
    }

    let pluck_tip = repo.get_ref("refs/heads/pluck/preload_many").unwrap();
    let count = repo.commit_count(&pluck_tip);
    assert_eq!(count, 6, "Should have 6 pluck commits");

    // Verify every source commit is still findable (full first-parent chain intact)
    for i in 1..=6 {
        let sha = repo.run_cmd("rev-parse", &[&format!("HEAD~{}", 6 - i)]);
        let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--find-pluck-sha", &sha]);
        let found = out.stdout.trim().to_string();
        assert_eq!(found.len(), 40, "Commit {} (sha {}): expected 40-char pluck SHA, got '{}'", i, sha, found);
        assert!(repo.is_reachable(&found, &pluck_tip), "Commit {} pluck SHA should be reachable", i);
    }
}
