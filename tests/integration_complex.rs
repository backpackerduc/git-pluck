use test_utils::TestRepo;

const TEST_CONFIG: &str = include_str!("testconfig");

struct HistoryShas {
    source_tip: String,
    merge_f3_sha: String,
}

/// Build a complex git history mirroring (49 source commits across
// branches branch_1-branch_7, orphan branch_8, and an octopus merge) and return key SHAs.
fn build_complex_history(repo: &TestRepo) -> HistoryShas {
    let base_ts = 100_000_000_i64;
    let date = format!("{base_ts} +0000");
    let date_env = &[("GIT_AUTHOR_DATE", date.as_str()), ("GIT_COMMITTER_DATE", date.as_str()), ("TZ", "UTC")];
    let head = || repo.run_cmd("rev-parse", &["HEAD"]);

    // --- master: commit_0_1 -> commit_0_2 -> commit_0_3 -> commit_0_4 -> commit_0_5 ---
    repo.write_file("foo", "commit_0_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_1"], date_env);

    repo.write_file("lib/lib.rs", "commit_0_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_2 [ lib ]"], date_env);

    repo.write_file("foo", "commit_0_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_3 [ ]"], date_env);
    let v2_sha = head();

    repo.write_file("lib/branch0_4.txt", "branch0_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_4 [ lib ]"], date_env);

    repo.write_file("lib/branch0_5.txt", "branch0_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_5 [ lib ]"], date_env);

    // --- branch_1 from commit_0_3: commit_1_1, commit_1_2, commit_1_3, commit_1_4, commit_1_5 ---
    repo.create_branch("branch_1", &v2_sha);
    repo.checkout("branch_1");
    repo.write_file("lib/lib.rs", "commit_1_1");
    repo.write_file("tool/tool.py", "commit_1_1");
    repo.write_file("doc/doc.md", "commit_1_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_1: commit_1_1 [ lib tool doc ]"], date_env);
    repo.write_file("foo", "commit_1_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_1: commit_1_2 [ ]"], date_env);
    repo.write_file("doc/branch1_3.txt", "branch1_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_1: commit_1_3 [ doc ]"], date_env);
    repo.write_file("other/branch1_4.txt", "branch1_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_1: commit_1_4 [ ]"], date_env);
    repo.write_file("lib/branch1_5.txt", "branch1_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_1: commit_1_5 [ lib ]"], date_env);

    // Merge branch_1 into master
    repo.checkout("master");
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_1' into master", "-X", "theirs", "branch_1"],
        date_env,
    );
    let merge_f1_sha = head();

    // --- branch_2 from merge-branch_1: commit_2_1, commit_2_2 ---
    repo.create_branch("branch_2", &merge_f1_sha);
    repo.checkout("branch_2");
    repo.write_file("foo", "commit_2_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_2: commit_2_1 [ ]"], date_env);
    repo.write_file("lib/branch2_2.txt", "branch4_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_2: commit_2_2 [ lib ]"], date_env);
    repo.run_cmd("rm", &["lib/branch2_2.txt"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_2: commit_2_3 [ -lib ]"], date_env);

    repo.checkout("master");
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_2' into master", "-X", "theirs", "branch_2"],
        date_env,
    );

    // --- branch_3 from commit_0_3: commit_3_1, commit_3_2, commit_3_3, commit_3_4, commit_3_5 ---
    repo.create_branch("branch_3", &v2_sha);
    repo.checkout("branch_3");
    repo.write_file("foo", "commit_3_1");
    repo.write_file("doc/doc.md", "commit_3_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_3: commit_3_1 [ doc ]"], date_env);
    repo.write_file("lib/lib.rs", "commit_3_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_3: commit_3_2 [ lib ]"], date_env);
    repo.write_file("lib/lib.rs", "commit_3_3");
    repo.write_file("tool/tool.py", "commit_3_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_3: commit_3_3 [ lib tool ]"], date_env);
    repo.write_file("other/branch4_4.txt", "branch4_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_3: commit_3_4 [ ]"], date_env);
    repo.write_file("lib/branch4_5.txt", "branch4_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_3: commit_3_5 [ lib ]"], date_env);

    repo.checkout("master");
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_3' into master", "-X", "theirs", "branch_3"],
        date_env,
    );

    // --- branch_4 from merge-branch_1: commit_4_1, commit_4_2, commit_4_3 ---
    repo.create_branch("branch_4", &merge_f1_sha);
    repo.checkout("branch_4");
    repo.write_file("foo", "commit_4_1");
    repo.write_file("doc/doc.md", "commit_4_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_4: commit_4_1 [ doc ]"], date_env);
    repo.write_file("lib/lib.rs", "commit_4_2");
    repo.write_file("doc/doc.md", "commit_4_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_4: commit_4_2 [ lib doc ]"], date_env);
    repo.write_file("lib/branch4_3.txt", "branch4_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_4: commit_4_3 [ lib ]"], date_env);

    repo.checkout("master");
    repo.write_file("other/master.txt", "master_6");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_6 [ ]"], date_env);
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_4' into master", "-X", "theirs", "branch_4"],
        date_env,
    );
    repo.write_file("lib/master.txt", "master_7");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_7 [ lib ]"], date_env);

    // --- branch_5 from merge-branch_1: commit_5_1, commit_5_2, commit_5_3, commit_5_4, commit_5_5, commit_5_6 ---
    repo.create_branch("branch_5", &merge_f1_sha);
    repo.checkout("branch_5");
    repo.write_file("lib/lib.rs", "commit_5_1");
    repo.write_file("tool/tool.py", "commit_5_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_5: commit_5_1 [ lib tool ]"], date_env);
    repo.write_file("foo", "commit_5_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_5: commit_5_2 [ ]"], date_env);
    repo.write_file("other/doc.md", "commit_5_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_5: commit_5_3 [ ]"], date_env);
    repo.write_file("foo", "commit_5_4");
    repo.write_file("other/tool.py", "commit_5_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_5: commit_5_4 [ ]"], date_env);
    repo.write_file("doc/branch5_5.txt", "branch5_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_5: commit_5_5 [ doc ]"], date_env);

    repo.checkout("master");
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_5' into master", "-X", "theirs", "branch_5"],
        date_env,
    );
    let merge_f3_sha = head();
    repo.write_file("other/master.txt", "master_8");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_8 [ ]"], date_env);
    repo.write_file("other2/master.txt", "master_9");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "master: commit_0_9 [ ]"], date_env);

    // --- branch_6 from merge-branch_5: commit_6_1, commit_6_2, commit_6_3, commit_6_4, commit_6_5, commit_6_6 ---
    repo.create_branch("branch_6", &merge_f3_sha);
    repo.checkout("branch_6");
    repo.write_file("lib/lib.rs", "commit_6_1");
    repo.write_file("tool/tool.py", "commit_6_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_6: commit_6_1 [ lib tool ]"], date_env);
    repo.write_file("foo", "commit_6_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_6: commit_6_2 [ ]"], date_env);
    repo.write_file("other/lib.rs", "commit_6_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_6: commit_6_3 [ ]"], date_env);
    repo.write_file("lib/branch6_4.txt", "branch6_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_6: commit_6_4 [ lib ]"], date_env);
    repo.write_file("tool/branch6_5.txt", "branch6_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_6: commit_6_5 [ tool ]"], date_env);

    // --- branch_7 from merge-branch_5: commit_7_1, commit_7_2 ---
    repo.create_branch("branch_7", &merge_f3_sha);
    repo.checkout("branch_7");
    repo.write_file("foo", "commit_7_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_7: commit_7_1 [ ]"], date_env);
    repo.write_file("other/branch8_2.txt", "branch8_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_7: commit_7_2 [ ]"], date_env);

    // Merge branch_7 into branch_6
    repo.checkout("branch_6");
    repo.run_cmd_with_env(
        "merge",
        &["--no-ff", "-m", "Merge 'branch_7' into branch_6", "-X", "theirs", "branch_7"],
        date_env,
    );

    // --- branch_8: orphan branch ---
    repo.run_cmd("checkout", &["--orphan", "branch_8"]);
    repo.run_cmd("rm", &["-rf", "."]);
    repo.write_file("foo", "commit_8_1");
    repo.write_file("lib/lib.rs", "commit_8_1");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_1 [ lib ]"], date_env);
    repo.write_file("lib/lib.rs", "commit_8_2");
    repo.write_file("tool/tool.py", "commit_8_2");
    repo.write_file("doc/doc.md", "commit_8_2");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_2 [ lib tool doc ]"], date_env);
    repo.write_file("foo", "commit_8_3");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_3 [ ]"], date_env);
    repo.write_file("tool/tool.py", "commit_8_4");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_4 [ tool ]"], date_env);
    repo.write_file("other/branch7_5.txt", "branch7_5");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_5 [ ]"], date_env);
    repo.write_file("doc/branch7_6.txt", "branch7_6");
    repo.run_cmd("add", &["-A"]);
    repo.run_cmd_with_env("commit", &["-m", "branch_8: commit_8_6 [ doc ]"], date_env);

    // --- Octopus merge branch_6 and branch_8 into master ---
    repo.checkout("master");
    // branch_8 is an orphan branch with unrelated history, so the merge fails.
    // It sets up the index and parents correctly, we just need to commit.
    let mut merge_cmd = std::process::Command::new("git");
    merge_cmd
        .args([
            "merge",
            "--no-ff",
            "-m",
            "Merge 'branch_6' and 'branch_8' into master",
            "--allow-unrelated-histories",
            "--strategy",
            "octopus",
            "--no-edit",
            "branch_6",
            "branch_8",
        ])
        .current_dir(repo.path())
        .envs(date_env.iter().map(|(k, v)| (*k, *v)));
    let _merge_output = merge_cmd.output().unwrap();
    // Finish the merge commit that git left in a pending state
    repo.run_cmd_with_env("commit", &["-m", "Merge 'branch_6' and 'branch_8' into master"], date_env);

    let source_tip = head();
    HistoryShas { source_tip, merge_f3_sha }
}

#[test]
fn test_recursive_complex_history() {
    let repo = TestRepo::new();
    let shas = build_complex_history(&repo);

    // ---- Run A: pluck full history in one go ----
    let config_a = repo.create_config("pluck_a", TEST_CONFIG);
    repo.run_pluck_ok(&["-c", config_a.to_str().unwrap(), "-s", &shas.source_tip]);
    let tip_a = repo.get_ref("refs/heads/pluck/pluck_a").expect("Pluck branch A should exist");
    let count_a = repo.commit_count(&tip_a);
    let msgs_a = repo.commit_messages(&tip_a);
    let files_a = repo.ls_tree(&tip_a);

    // ---- Run B: pluck in two steps (partial, then full) ----
    let config_b = repo.create_config("pluck_b", TEST_CONFIG);

    // Step 1: pluck up to merge-branch_5 (about halfway through history)
    repo.run_pluck_ok(&["-c", config_b.to_str().unwrap(), "-s", &shas.merge_f3_sha]);

    // Step 2: pluck the full history (should pick up only new commits)
    repo.run_pluck_ok(&["-c", config_b.to_str().unwrap(), "-s", &shas.source_tip]);
    let tip_b = repo.get_ref("refs/heads/pluck/pluck_b").expect("Pluck branch B should exist");

    // ---- Compare: both should yield identical results ----
    assert_eq!(tip_a, tip_b);
    // c71065020f8edced447ab15dd21e27b73447461f extracted in a manual test
    assert_eq!(tip_a, "41532937c81c3679a60bb8c25c4a20977d5824cc");

    // ---- Verify ground truth ----
    let assert_plucked = |msg_hint: &str| {
        assert!(
            msgs_a.iter().any(|m| m.contains(msg_hint)),
            "Expected commit containing '{}' to be plucked. Messages: {:?}",
            msg_hint,
            msgs_a
        );
    };
    let assert_not_plucked = |msg_hint: &str| {
        assert!(
            !msgs_a.iter().any(|m| m.contains(msg_hint)),
            "Expected commit containing '{}' to NOT be plucked. Messages: {:?}",
            msg_hint,
            msgs_a
        );
    };

    // Should be plucked
    assert_plucked("master: commit_0_2");
    assert_plucked("branch_1: commit_1_1");
    assert_plucked("Merge 'branch_1' into master");
    assert_plucked("branch_3: commit_3_1");
    assert_plucked("branch_3: commit_3_2");
    assert_plucked("branch_3: commit_3_3");
    assert_plucked("Merge 'branch_3' into master");
    assert_plucked("branch_4: commit_4_1");
    assert_plucked("branch_4: commit_4_2");
    assert_plucked("Merge 'branch_4' into master");
    assert_plucked("branch_5: commit_5_1");
    assert_plucked("Merge 'branch_5' into master");
    assert_plucked("branch_6: commit_6_1");
    assert_plucked("branch_8: commit_8_1");
    assert_plucked("branch_8: commit_8_2");
    assert_plucked("branch_8: commit_8_4");
    assert_plucked("Merge 'branch_6' and 'branch_8' into master");

    // Should NOT be plucked
    assert_not_plucked("master: commit_0_1");
    assert_not_plucked("master: commit_0_3");
    assert_not_plucked("branch_1: commit_1_2");
    assert_not_plucked("branch_2: commit_2_1");
    assert_not_plucked("Merge 'branch_2' into master");
    assert_not_plucked("branch_5: commit_5_2");
    assert_not_plucked("branch_6: commit_6_2");
    assert_not_plucked("branch_7: commit_7_1");
    assert_not_plucked("Merge 'branch_7' into 'branch_6'");
    assert_not_plucked("branch_8: commit_8_3");

    // Verify the tip has the right files (mapped destinations, not sources)
    assert!(
        files_a.iter().any(|f| f.starts_with("lib/li/")),
        "lib/l/ should exist (mapped from lib/). Files: {:?}",
        files_a
    );
    assert!(
        files_a.iter().any(|f| f.starts_with("lib/to/")),
        "lib/t/ should exist (mapped from tool/). Files: {:?}",
        files_a
    );
    assert!(
        files_a.iter().any(|f| f.starts_with("lib/do/")),
        "lib/do/ should exist (mapped from doc/). Files: {:?}",
        files_a
    );
    assert!(
        !files_a.iter().any(|f| f.starts_with("doc/") || f == "doc/doc.md"),
        "Source doc/ should not exist. Files: {:?}",
        files_a
    );
    assert!(
        !files_a.iter().any(|f| f.starts_with("tool/") || f == "tool/tool.py"),
        "Source tool/ should not exist. Files: {:?}",
        files_a
    );
    assert!(
        !files_a.iter().any(|f| f.starts_with("lib/")
            && !f.starts_with("lib/li/")
            && !f.starts_with("lib/to/")
            && !f.starts_with("lib/do/")),
        "Source lib/ (mapped to different order) should not exist. Files: {:?}",
        files_a
    );
    assert!(!files_a.iter().any(|f| f == "foo"), "foo (unmapped) should not exist. Files: {:?}", files_a);

    // Verify merge topology: the octopus merge pluck commit should have multiple parents
    let pluck_log = repo.run_cmd("log", &["--format=%P %s", &tip_a]);
    let octopus_line = pluck_log
        .lines()
        .find(|l| l.contains("Merge 'branch_6' and 'branch_8' into master"))
        .expect("Octopus merge should be in pluck log");
    let octopus_parents: Vec<&str> = octopus_line.split_whitespace().take_while(|s| s.len() == 40).collect();
    assert!(
        octopus_parents.len() >= 2,
        "Octopus merge pluck commit should have >= 2 parents, got {}. Line: {}",
        octopus_parents.len(),
        octopus_line
    );

    eprintln!("Pluck branch tip: {}", tip_a);
    eprintln!("Pluck commit count: {}", count_a);
}
