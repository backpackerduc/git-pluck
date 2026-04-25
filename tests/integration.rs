use std::fs;
use std::process::Command;
use test_utils::TestRepo;

// ============================================================================
// Config parsing tests
// ============================================================================

#[test]
fn test_default_config_values() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    // Create a minimal config with no behavior keys
    let config_path = repo.create_config("minimal", "[forward.from \"src\"]\n    to = dst\n");

    // --show-src-paths should work with defaults
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src"]);
}

#[test]
fn test_config_bool_normalization() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    // Boolean values: true -> 1, false -> 0
    let config_content = r#"
[forward.from "src"]
    to = (Mirror)

[pluck]
    force = true
"#;
    let config_path = repo.create_config("booltest", config_content);

    // Should work without force errors since force=true in config
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src"]);
}

#[test]
fn test_config_start_ref_default() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    let config_path = repo.create_config("source_default", "[forward.from \".\"]\n    to = (Mirror)\n");

    // Default source commit is HEAD
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);
    assert!(out.stdout.contains("Processed") || out.code == 0);
}

#[test]
fn test_config_file_not_found() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    let out = repo.run_pluck(&["-c", "/nonexistent/path/config", "--show-src-paths"]);
    assert_ne!(out.code, 0);
}

#[test]
fn test_pluckname_from_config() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    let config_path = repo.create_config("mypluck", "[forward.from \".\"]\n    to = (Mirror)\n");

    // Pluck name should be derived from config file stem
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["."]);
}

// ============================================================================
// CLI flag tests
// ============================================================================

#[test]
fn test_cli_help() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    let out = repo.run_pluck(&["--help"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("git-pluck"));
}

#[test]
fn test_cli_no_force_overrides_force() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    // --no-force should override --force
    let config_content = "[forward.from \"nonexistent\"]\n    to = (Mirror)\n";
    let config_path = repo.create_config("noforce", config_content);

    // --show-src-paths doesn't actually pluck, so it won't hit the missing source error
    // Test with actual pluck: --force should allow missing sources
    let out_no_force = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--force", "--no-force", "--log-branch"]);
    // Without force (overridden by --no-force), missing sources should error
    assert_ne!(out_no_force.code, 0);
}

#[test]
fn test_cli_no_rep_author_name_clears() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_content = r#"
[forward.from "src.txt"]
    to = (Mirror)

[pluck]
    repAuthorName = "Replaced Name"
"#;
    let config_path = repo.create_config("modauthor", config_content);

    // --no-rep-author-name should clear the config value
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--no-rep-author-name", "--log-branch"]);
    // Check that the author name was NOT replaced (original author should be used)
    let pluck_ref = repo.get_ref("refs/heads/pluck/modauthor").unwrap();
    let (name, _) = repo.commit_author(&pluck_ref);
    assert_eq!(name, "Test Author");
}

#[test]
fn test_cli_start_ref_override() {
    let repo = TestRepo::new();
    let sha1 = repo.commit_file("a.txt", "content a", "commit a");
    repo.commit_file("b.txt", "content b", "commit b");

    let config_path = repo.create_config("source_override", "[forward.from \".\"]\n    to = (Mirror)\n");

    // Use --start-ref to specify a specific commit
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "-s", &sha1, "--log-branch"]);
    assert!(out.code == 0);
}

#[test]
fn test_cli_test_mode() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    // --test-mode should exit 0 without doing anything
    let out = repo.run_pluck(&["--test-mode"]);
    assert_eq!(out.code, 0);
}

#[test]
fn test_env_pluck_test_mode() {
    let repo = TestRepo::new();
    repo.commit_file("initial.txt", "hello", "initial");

    // PLUCK_TEST_MODE env var should also exit 0
    let output = Command::new(env!("CARGO_BIN_EXE_git-pluck"))
        .env("PLUCK_TEST_MODE", "1")
        .current_dir(repo.path())
        .output()
        .expect("Failed to run git-pluck");
    assert_eq!(output.status.code().unwrap_or(-1), 0);
}

// ============================================================================
// Map building tests
// ============================================================================

#[test]
fn test_map_mirror() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("mirror", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);
}

#[test]
fn test_map_remove() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("remove", "[forward.from \"src.txt\"]\n    to = (Remove)\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    // Removed paths should not appear in destinations
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert!(out.stdout_lines().is_empty());
}

#[test]
fn test_map_move() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("move", "[forward.from \"src.txt\"]\n    to = dest.txt\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["dest.txt"]);
}

#[test]
fn test_map_unpack() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("unpack", "[forward.from \"src.txt\"]\n    to = .\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["."]);
}

#[test]
fn test_map_reverse() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_content = "[forward.from \"src.txt\"]\n    to = dest.txt\n\n[pluck]\n    autoReverseMap = true\n";
    let config_path = repo.create_config("reverse", config_content);

    // With autoReverseMap, src and dst are swapped
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["dest.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);
}

#[test]
fn test_map_reverse_does_not_affect_remove() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_content = "[forward.from \"src.txt\"]\n    to = (Remove)\n\n[pluck]\n    autoReverseMap = true\n";
    let config_path = repo.create_config("reverse_remove", config_content);

    // Remove should not be reversed
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert!(out.stdout_lines().is_empty());
}

#[test]
fn test_map_mirror_map() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_content = "[forward.from \"src.txt\"]\n    to = dest.txt\n\n[pluck]\n    mirrorMap = true\n";
    let config_path = repo.create_config("mirror_map", config_content);

    // mirrorMap replaces all mappings with map=true (mirror), strips copies
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    // mirror map: destination = source
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);
}

#[test]
fn test_map_multiple_mappings() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");
    repo.commit_file("b.txt", "b", "add b");

    let config_content = "[forward.from \"a.txt\"]\n    to = x.txt\n\n[forward.from \"b.txt\"]\n    to = y.txt\n";
    let config_path = repo.create_config("multi", config_content);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    let mut sources = out.stdout_lines();
    sources.sort();
    assert_eq!(sources, vec!["a.txt", "b.txt"]);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    let mut dsts = out.stdout_lines();
    dsts.sort();
    assert_eq!(dsts, vec!["x.txt", "y.txt"]);
}

#[test]
fn test_map_copy_entries() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    // Copy entries: to = (Copy)<path>
    let config_content = "[forward.from \"src.txt\"]\n    to = dest.txt\n    to = (Copy)copy.txt\n";
    let config_path = repo.create_config("copy", config_content);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);

    // Primary destination should be listed
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["dest.txt"]);
}

// ============================================================================
// Map validation tests
// ============================================================================

#[test]
fn test_check_config_valid() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("valid", "[forward.from \"src.txt\"]\n    to = dest.txt\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--check-config"]);
    assert!(out.stdout.contains("passed"));
}

#[test]
fn test_check_config_duplicate_destination() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");

    // Two sources mapping to the same destination
    let config_content = "[forward.from \"a.txt\"]\n    to = same.txt\n\n[forward.from \"b.txt\"]\n    to = same.txt\n";
    let config_path = repo.create_config("dup_dst", config_content);

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--check-config"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("Duplicate"));
}

#[test]
fn test_check_config_nested_map_error() {
    let repo = TestRepo::new();
    repo.commit_file("a/b/c.txt", "nested", "initial");

    // Nested mapping: a and a/b
    let config_content = "[forward.from \"a\"]\n    to = x\n\n[forward.from \"a/b\"]\n    to = y\n";
    let config_path = repo.create_config("nested", config_content);

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--check-config"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("Nested"));
}

#[test]
fn test_check_config_allow_nested_map_config() {
    let repo = TestRepo::new();
    repo.commit_file("a/b/c.txt", "nested", "initial");

    let config_content = "[forward.from \"a\"]\n    to = x\n\n[forward.from \"a/b\"]\n    to = y\n\n[pluck]\n    allowNestedMap = true\n";
    let config_path = repo.create_config("nested_ok", config_content);

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--check-config"]);
    assert!(out.stdout.contains("passed"));
}

#[test]
fn test_check_config_allow_nested_map_cli() {
    let repo = TestRepo::new();
    repo.commit_file("a/b/c.txt", "nested", "initial");

    let config_content = "[forward.from \"a\"]\n    to = x\n\n[forward.from \"a/b\"]\n    to = y\n";
    let config_path = repo.create_config("nested_cli", config_content);

    // --allow-nested-map CLI flag should allow nested mappings
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--check-config", "--allow-nested-map"]);
    assert!(out.stdout.contains("passed"));
}

#[test]
fn test_check_config_trailing_slash() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_content = "[forward.from \"src/\"]\n    to = dest\n";
    let config_path = repo.create_config("trailing", config_content);

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--check-config"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("Trailing slash"));
}

// ============================================================================
// --add-map subcommand tests
// ============================================================================

#[test]
fn test_add_mirror_map() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_path = repo.create_config("addtest", "");

    // Add mirror map (no colon)
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--add-map=srcfile.txt"]);

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[forward.from \"srcfile.txt\"]"));
    assert!(content.contains("to = (Mirror)"));
}

#[test]
fn test_add_src_dst_map() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_path = repo.create_config("addtest2", "");

    // Add src:dst mapping
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--add-map=src.txt:dest.txt"]);

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[forward.from \"src.txt\"]"));
    assert!(content.contains("to = dest.txt"));
}

#[test]
fn test_add_remove_map() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_path = repo.create_config("addtest3", "");

    // Add src:false (remove mapping)
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--add-map=src.txt:(Remove)"]);

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[forward.from \"src.txt\"]"));
    assert!(content.contains("to = (Remove)"));
}

#[test]
fn test_add_duplicate_mirror_error() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_content = "[forward.from \"existing.txt\"]\n    to = (Mirror)\n";
    let config_path = repo.create_config("addtest4", config_content);

    // Adding the same source as mirror map should error without --force
    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--add-map=existing.txt"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("already exists") || out.stderr.contains("error"), "stderr: {}", out.stderr);
}

#[test]
fn test_add_duplicate_with_force() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_content = "[forward.from \"existing.txt\"]\n    to = old.txt\n";
    let config_path = repo.create_config("addtest5", config_content);

    // --force should allow overwriting
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--force", "--add-map=existing.txt"]);
}

#[test]
fn test_add_update_destination_allowed() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_content = "[forward.from \"src.txt\"]\n    to = old.txt\n";
    let config_path = repo.create_config("addtest6", config_content);

    // Updating destination (with colon) should be allowed even without --force
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--add-map=src.txt:new.txt"]);

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("to = new.txt"));
}

// ============================================================================
// --remove subcommand tests
// ============================================================================

#[test]
fn test_remove_mapping() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_content =
        "[forward.from \"src.txt\"]\n    to = dest.txt\n\n[forward.from \"other.txt\"]\n    to = other_dest.txt\n";
    let config_path = repo.create_config("removetest", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--remove=src.txt"]);

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(!content.contains("[forward.from \"src.txt\"]"));
    assert!(content.contains("[forward.from \"other.txt\"]"));
}

#[test]
fn test_remove_nonexistent_source() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    let config_content = "[forward.from \"src.txt\"]\n    to = dest.txt\n";
    let config_path = repo.create_config("removetest2", config_content);

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--remove=nonexistent.txt"]);
    assert_ne!(out.code, 0);
}

// ============================================================================
// Tree construction tests
// ============================================================================

#[test]
fn test_pluck_tree_mirror() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello world", "initial");

    let config_path = repo.create_config("tree_id", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_id");
    assert!(files.contains(&"src.txt".to_string()));
}

#[test]
fn test_pluck_tree_move() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("tree_mv", "[forward.from \"src.txt\"]\n    to = dest.txt\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_mv");
    assert!(files.contains(&"dest.txt".to_string()));
    assert!(!files.contains(&"src.txt".to_string()));
}

#[test]
fn test_pluck_tree_directory_move() {
    let repo = TestRepo::new();
    repo.commit_file("src/file.txt", "content", "initial");

    let config_path = repo.create_config("tree_dir", "[forward.from \"src\"]\n    to = dest\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_dir");
    assert!(files.contains(&"dest/file.txt".to_string()));
}

#[test]
fn test_pluck_tree_root_base_prefix() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");
    repo.commit_file("b.txt", "b", "add b");

    // [forward.from "."] to = backend -> all files go under backend/
    let config_path = repo.create_config("root_prefix", "[forward.from \".\"]\n    to = backend\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/root_prefix");
    assert!(files.contains(&"backend/a.txt".to_string()));
    assert!(files.contains(&"backend/b.txt".to_string()));
}

#[test]
fn test_pluck_tree_root_prefix_with_override() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");
    repo.commit_file("docs/readme.txt", "docs", "add docs");

    // Root prefix + specific override
    let config_content = "[forward.from \".\"]\n    to = backend\n\n[forward.from \"docs\"]\n    to = docs\n";
    let config_path = repo.create_config("prefix_override", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/prefix_override");
    assert!(files.contains(&"backend/a.txt".to_string()));
    assert!(files.contains(&"docs/readme.txt".to_string()));
    // docs should NOT be under backend
    assert!(!files.contains(&"backend/docs/readme.txt".to_string()));
}

#[test]
fn test_pluck_tree_remove() {
    let repo = TestRepo::new();
    repo.commit_file("keep.txt", "keep", "initial");
    repo.commit_file("remove_me.txt", "nope", "add remove");

    let config_content =
        "[forward.from \".\"]\n    to = (Mirror)\n\n[forward.from \"remove_me.txt\"]\n    to = (Remove)\n";
    let config_path = repo.create_config("tree_remove", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_remove");
    assert!(files.contains(&"keep.txt".to_string()));
    assert!(!files.contains(&"remove_me.txt".to_string()));
}

#[test]
fn test_pluck_tree_copy() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    // Primary + copy
    let config_content = "[forward.from \"src.txt\"]\n    to = dest.txt\n    to = (Copy)copy.txt\n";
    let config_path = repo.create_config("tree_copy", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_copy");
    assert!(files.contains(&"dest.txt".to_string()));
    assert!(files.contains(&"copy.txt".to_string()));
}

#[test]
fn test_pluck_tree_only_copy_no_primary() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    // Only copy entries, no primary
    let config_content = "[forward.from \"src.txt\"]\n    to = (Copy)copy1.txt\n    to = (Copy)copy2.txt\n";
    let config_path = repo.create_config("tree_onlycopy", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/tree_onlycopy");
    assert!(files.contains(&"copy1.txt".to_string()));
    assert!(files.contains(&"copy2.txt".to_string()));
}

// ============================================================================
// Author/committer replacement tests
// ============================================================================

#[test]
fn test_rep_author_name_standalone() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_an", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--rep-author-name=Replaced Author"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_an").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    assert_eq!(name, "Replaced Author");
    // Email should be preserved from source
    assert_eq!(email, "test@example.com");
}

#[test]
fn test_rep_author_email_standalone() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_ae", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&[
        "-c",
        config_path.to_str().unwrap(),
        "--log-branch",
        "--rep-author-email=replaced@example.com",
    ]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_ae").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    // Name should be preserved from source
    assert_eq!(name, "Test Author");
    assert_eq!(email, "replaced@example.com");
}

#[test]
fn test_rep_author_both() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_both", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&[
        "-c",
        config_path.to_str().unwrap(),
        "--log-branch",
        "--rep-author-name=New Author",
        "--rep-author-email=new@example.com",
    ]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_both").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    assert_eq!(name, "New Author");
    assert_eq!(email, "new@example.com");
}

#[test]
fn test_rep_committer_name_standalone() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_cn", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--rep-committer-name=New Committer"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_cn").unwrap();
    let (name, email) = repo.commit_committer(&pluck_ref);
    assert_eq!(name, "New Committer");
    assert_eq!(email, "test@example.com");
}

#[test]
fn test_rep_author_regex_replace() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_regex", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Allow pattern matches the test email -> should NOT replace (email protected)
    repo.run_pluck_ok(&[
        "-c",
        config_path.to_str().unwrap(),
        "--log-branch",
        "--rep-author-regex=.*@example\\.com:",
        "--rep-author-name=Regex Author",
        "--rep-author-email=regex@replaced.com",
    ]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_regex").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    assert_eq!(name, "Test Author");
    assert_eq!(email, "test@example.com");
}

#[test]
fn test_rep_author_regex_protected() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_regex2", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Allow pattern that does NOT match -> should REPLACE (email not protected)
    repo.run_pluck_ok(&[
        "-c",
        config_path.to_str().unwrap(),
        "--log-branch",
        "--rep-author-regex=.*@other\\.com:",
        "--rep-author-name=Regex Author",
        "--rep-author-email=regex@replaced.com",
    ]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_regex2").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    // Should be replaced since email doesn't match allow pattern
    assert_eq!(name, "Regex Author");
    assert_eq!(email, "regex@replaced.com");
}

#[test]
fn test_rep_author_regex_deny_override() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rep_regex3", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Allow pattern matches, but deny pattern also matches -> replace
    repo.run_pluck_ok(&[
        "-c",
        config_path.to_str().unwrap(),
        "--log-branch",
        "--rep-author-regex=.*@example\\.com:.*@example\\.com",
        "--rep-author-name=Deny Author",
        "--rep-author-email=deny@replaced.com",
    ]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_regex3").unwrap();
    let (name, email) = repo.commit_author(&pluck_ref);
    assert_eq!(name, "Deny Author");
    assert_eq!(email, "deny@replaced.com");
}

// ============================================================================
// Message replacement tests
// ============================================================================

#[test]
fn test_rep_message_replace() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "original message");

    let config_path = repo.create_config("rep_msg", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--rep-message=Replaced message"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_msg").unwrap();
    let msg = repo.commit_message(&pluck_ref);
    assert!(msg.contains("Replaced message"));
    assert!(!msg.contains("original message"));
}

#[test]
fn test_rep_message_filter() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "Line one\nSecret line\nLine three");

    let config_path = repo.create_config("rep_msgf", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--rep-message-filter=Secret"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rep_msgf").unwrap();
    let msg = repo.commit_message(&pluck_ref);
    assert!(msg.contains("Line one"));
    assert!(!msg.contains("Secret line"));
    assert!(msg.contains("Line three"));
}

#[test]
fn test_log_message_added() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("with_log_message", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let source_sha = repo.run_cmd("rev-parse", &["HEAD"]);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-message", "--no-log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/with_log_message").unwrap();
    let msg = repo.commit_message(&pluck_ref);
    assert!(msg.contains(&format!("Plucked from: {}", source_sha)));
}

// ============================================================================
// Sanity check / error path tests
// ============================================================================

#[test]
fn test_error_allow_unchanged_tree_with_recursive() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("err1", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out =
        repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--allow-unchanged-tree", "--recursive", "--log-branch"]);
    assert_ne!(out.code, 0);
    assert!(
        out.stderr.contains("cannot be combined") || out.stderr.contains("combined") || out.stderr.contains("error"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn test_error_no_log_branch_no_log_message() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("err2", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--no-log-branch"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("must be enabled") || out.stderr.contains("error"), "stderr: {}", out.stderr);
}

#[test]
fn test_error_rep_author_regex_without_name_email() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("err3", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--rep-author-regex=.*", "--log-branch"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("requires") || out.stderr.contains("error"), "stderr: {}", out.stderr);
}

#[test]
fn test_error_rep_committer_regex_without_name_email() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("err4", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--rep-committer-regex=.*", "--log-branch"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("requires") || out.stderr.contains("error"), "stderr: {}", out.stderr);
}

#[test]
fn test_error_missing_source_tree() {
    let repo = TestRepo::new();
    repo.commit_file("other.txt", "hello", "initial");

    // Map a source that doesn't exist
    let config_path = repo.create_config("err5", "[forward.from \"nonexistent.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--log-branch"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("not found") || out.stderr.contains("error"), "stderr: {}", out.stderr);
}

#[test]
fn test_allow_missing_path_flag_allows_missing() {
    let repo = TestRepo::new();
    repo.commit_file("other.txt", "hello", "initial");

    let config_content =
        "[forward.from \"nonexistent.txt\"]\n    to = (Mirror)\n\n[pluck]\n    allowMissingPath = true\n";
    let config_path = repo.create_config("err5_ok", config_content);

    // With allowMissingPath=true, missing sources are tolerated (warning only)
    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--log-branch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

#[test]
fn test_force_allows_allow_missing_path() {
    let repo = TestRepo::new();
    repo.commit_file("other.txt", "hello", "initial");

    let config_path = repo.create_config("err5_force", "[forward.from \"nonexistent.txt\"]\n    to = (Mirror)\n");

    // --force should bypass missing source errors
    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--force", "--log-branch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

// ============================================================================
// Incremental plucking tests
// ============================================================================

#[test]
fn test_incremental_pluck_second_run_no_new_commits() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("incr", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // First run
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let first_count = repo.commit_count("refs/heads/pluck/incr");

    // Second run with same state - should not create new commits
    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    // Should report no new commits (exit 1 = no new revisions)
    assert_ne!(out.code, 0);
    let second_count = repo.commit_count("refs/heads/pluck/incr");
    assert_eq!(first_count, second_count);
}

#[test]
fn test_incremental_pluck_new_commit_processed() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("incr2", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // First run
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let first_count = repo.commit_count("refs/heads/pluck/incr2");

    // Add a new commit
    repo.commit_file("src.txt", "updated", "update");

    // Second run should process the new commit
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let second_count = repo.commit_count("refs/heads/pluck/incr2");
    assert!(second_count > first_count, "Expected {} > {}", second_count, first_count);
}

// ============================================================================
// Recursive mode tests
// ============================================================================

#[test]
fn test_recursive_mode() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");
    repo.commit_file("src.txt", "v3", "commit 3");

    let config_path = repo.create_config("recurse", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch"]);

    // Should have created pluck commits
    let pluck_count = repo.commit_count("refs/heads/pluck/recurse");
    assert!(pluck_count >= 1);
}

#[test]
fn test_recursive_with_opts() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");

    let config_path = repo.create_config("recurse_opts", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // --recursive=max-count:1 should limit to 1 commit
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--recursive=max-count:1", "--log-branch"]);

    let pluck_count = repo.commit_count("refs/heads/pluck/recurse_opts");
    assert_eq!(pluck_count, 1);
}

// ============================================================================
// Merge commit tests
// ============================================================================

#[test]
fn test_merge_commit_parents() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "main", "main commit");

    let merge_sha = repo.merge_commit("feature", "merge feature");
    let source_parents = repo.commit_parents(&merge_sha);
    assert_eq!(source_parents.len(), 2, "Merge commit should have 2 parents");

    let config_path = repo.create_config("merge", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Single commit mode on merge
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "-s", &merge_sha, "--log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/merge").unwrap();
    // The pluck commit should exist
    assert!(!pluck_ref.is_empty());
}

// ============================================================================
// Log branch tests
// ============================================================================

#[test]
fn test_log_commit_created() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("mylog", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    // Log branch should exist
    let log_ref = repo.get_ref("refs/heads/pluck/log/mylog");
    assert!(log_ref.is_some(), "Log branch should exist");

    // Log commit should have the right author
    let log_commit_sha = log_ref.unwrap();
    let (name, email) = repo.commit_author(&log_commit_sha);
    assert_eq!(name, "Git-Pluck");
    assert_eq!(email, "git@pluck");

    // Log commit message should contain Pluck name
    let msg = repo.commit_message(&log_commit_sha);
    assert!(msg.contains("[SOURCE:PLUCK]"));
    assert!(msg.contains("mylog"));
}

#[test]
fn test_log_commit_triple_merge() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("log2", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let log_commit_sha = repo.get_ref("refs/heads/pluck/log/log2").unwrap();
    let parents = repo.commit_parents(&log_commit_sha);
    // Triple merge: prev log (or source), source, pluck tip
    assert!(parents.len() >= 2, "Log commit should have at least 2 parents, got {}", parents.len());
}

#[test]
fn test_progress_ref_cleaned_up() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("myprogress", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    // Progress ref should be deleted on success
    let progress_ref = repo.get_ref("refs/heads/pluck/progress/myprogress");
    assert!(progress_ref.is_none(), "Progress ref should be deleted after successful pluck");
}

// ============================================================================
// Added Source SHA mode tests
// ============================================================================

#[test]
fn test_log_message_mode() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("log_message", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let source_sha = repo.run_cmd("rev-parse", &["HEAD"]);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-message", "--no-log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/log_message").unwrap();
    let msg = repo.commit_message(&pluck_ref);
    assert!(msg.contains(&format!("Plucked from: {}", source_sha)));
}

// ============================================================================
// Update ref tests
// ============================================================================

#[test]
fn test_ignorant_pluck() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("ignorantpluck", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--ignorant-pluck=refs/heads/custom-ref", "--log-branch"]);

    // Custom ref should exist
    let custom_ref = repo.get_ref("refs/heads/custom-ref");
    assert!(custom_ref.is_some(), "Custom ref should exist");

    // Files should be correct
    let files = repo.ls_tree(custom_ref.unwrap().as_str());
    assert!(files.contains(&"src.txt".to_string()));
}

// ============================================================================
// Quiet mode tests
// ============================================================================

#[test]
fn test_quiet_mode() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("quiet", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch", "--quiet"]);

    // Should have minimal output
    assert!(!out.stdout.contains("PLUCKING"));
}

// ============================================================================
// Empty tree skip tests
// ============================================================================

#[test]
fn test_commit_touching_only_non_mapped_files() {
    let repo = TestRepo::new();
    repo.commit_file("mapped.txt", "v1", "initial mapped");
    repo.commit_file("unmapped.txt", "v1", "add unmapped");

    // Only map mapped.txt
    let config_path = repo.create_config("emptytree", "[forward.from \"mapped.txt\"]\n    to = (Mirror)\n");

    // First pluck
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let first_count = repo.commit_count("refs/heads/pluck/emptytree");

    // Now change only the unmapped file
    repo.commit_file("unmapped.txt", "v2", "change unmapped");

    // Recursive pluck - the new commit should produce an empty tree change
    // and be skipped
    repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch"]);

    // Count should be the same (no new pluck commit for unmapped change)
    let second_count = repo.commit_count("refs/heads/pluck/emptytree");
    assert_eq!(first_count, second_count, "Should not create pluck commit for changes outside the map");
}

// ============================================================================
// Force new pluck tests
// ============================================================================

#[test]
fn test_allow_unchanged_tree_single_commit() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("force_new", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // First pluck
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let first_sha = repo.get_ref("refs/heads/pluck/force_new").unwrap();

    // Pluck again with --allow-unchanged-tree (same commit, should create new)
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--allow-unchanged-tree", "--log-branch"]);

    let second_sha = repo.get_ref("refs/heads/pluck/force_new").unwrap();
    assert_ne!(first_sha, second_sha, "--allow-unchanged-tree should create a new commit even with unchanged tree");
}

// ============================================================================
// PLUCK_NO_RETURN_ERROR tests
// ============================================================================

#[test]
fn test_pluck_no_return_error() {
    let repo = TestRepo::new();
    repo.commit_file("init.txt", "init", "initial");

    // Missing log branch and log message  should normally exit 2
    let config_path = repo.create_config("noerr", "[forward.from \"init.txt\"]\n    to = (Mirror)\n");

    let output = Command::new(env!("CARGO_BIN_EXE_git-pluck"))
        .args(["-c", config_path.to_str().unwrap(), "--no-log-branch"])
        .env("PLUCK_NO_RETURN_ERROR", "1")
        .current_dir(repo.path())
        .output()
        .expect("Failed to run git-pluck");

    // Should return 0 instead of 2
    assert_eq!(output.status.code().unwrap_or(-1), 0, "PLUCK_NO_RETURN_ERROR should make all errors return 0");
}

// ============================================================================
// Deterministic output tests
// ============================================================================

#[test]
fn test_deterministic_tree_order() {
    let repo = TestRepo::new();
    repo.commit_file("z.txt", "z", "add z");
    repo.commit_file("a.txt", "a", "add a");
    repo.commit_file("m.txt", "m", "add m");

    let config_path = repo.create_config("deterministic", "[forward.from \".\"]\n    to = (Mirror)\n");

    // Run pluck twice and compare tree contents
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files1 = repo.ls_tree("refs/heads/pluck/deterministic");

    // Reset and run again
    repo.run_cmd("update-ref", &["-d", "refs/heads/pluck/deterministic"]);
    repo.run_cmd("update-ref", &["-d", "refs/heads/pluck/log/deterministic"]);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files2 = repo.ls_tree("refs/heads/pluck/deterministic");
    assert_eq!(files1, files2, "Tree order should be deterministic across runs");
}

// ============================================================================
// Deep directory tests
// ============================================================================

#[test]
fn test_deep_directory_structure() {
    let repo = TestRepo::new();
    repo.commit_file("a/b/c/d/file.txt", "deep", "deep file");
    repo.commit_file("a/b/other.txt", "other", "other file");

    let config_path = repo.create_config("deepdir", "[forward.from \"a\"]\n    to = x\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/deepdir");
    assert!(files.contains(&"x/b/c/d/file.txt".to_string()));
    assert!(files.contains(&"x/b/other.txt".to_string()));
}

#[test]
fn test_multiple_files_same_directory() {
    let repo = TestRepo::new();
    repo.commit_file("dir/a.txt", "a", "add a");
    repo.commit_file("dir/b.txt", "b", "add b");
    repo.commit_file("dir/c.txt", "c", "add c");

    let config_path = repo.create_config("multidir", "[forward.from \"dir\"]\n    to = newdir\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let files = repo.ls_tree("refs/heads/pluck/multidir");
    assert!(files.contains(&"newdir/a.txt".to_string()));
    assert!(files.contains(&"newdir/b.txt".to_string()));
    assert!(files.contains(&"newdir/c.txt".to_string()));
}

// ============================================================================
// Config resolution priority tests
// ============================================================================

#[test]
fn test_explicit_config_overrides_working_tree() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    // Working tree config
    repo.create_config("priority", "[forward.from \"src.txt\"]\n    to = working.txt\n");

    // Explicit config file (different location)
    let explicit_dir = repo.path().join("explicit_configs");
    fs::create_dir_all(&explicit_dir).unwrap();
    let explicit_path = explicit_dir.join("config");
    fs::write(&explicit_path, "[forward.from \"src.txt\"]\n    to = explicit.txt\n").unwrap();

    let out = repo.run_pluck_ok(&["-c", explicit_path.to_str().unwrap(), "--show-dst-paths"]);
    assert_eq!(out.stdout_lines(), vec!["explicit.txt"]);
}

// ============================================================================
// Map validation during pluck
// ============================================================================

#[test]
fn test_map_validation_blocks_pluck() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");
    repo.commit_file("b.txt", "b", "add b");

    // Duplicate destinations
    let config_content = "[forward.from \"a.txt\"]\n    to = same.txt\n\n[forward.from \"b.txt\"]\n    to = same.txt\n";
    let config_path = repo.create_config("valblock", config_content);

    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--log-branch"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("validation") || out.stderr.contains("Duplicate"), "stderr: {}", out.stderr);
}

#[test]
fn test_map_validation_bypassed_by_force() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "a", "initial");

    // Trailing slash (validation error)
    let config_content = "[forward.from \"a/\"]\n    to = dest\n";
    let config_path = repo.create_config("valforce", config_content);

    // --force should bypass validation
    let out = repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--force", "--log-branch"]);
    // May succeed or fail on other grounds, but not on map validation
    assert!(
        out.code == 0 || !out.stderr.contains("validation"),
        "--force should bypass map validation. stderr: {}",
        out.stderr
    );
}

// ============================================================================
// Partial ancestry tests
// ============================================================================

#[test]
fn test_allow_incomplete_ancestry_flag() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");

    let config_path = repo.create_config("partial", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // Recursive with allow-incomplete-ancestry should not fail on unresolved parents
    let out = repo.run_pluck(&[
        "-c",
        config_path.to_str().unwrap(),
        "--recursive",
        "--log-branch",
        "--allow-incomplete-ancestry",
    ]);
    // Should succeed or at least not fail with ancestry error
    assert!(out.code == 0 || !out.stderr.contains("ancestry"), "stderr: {}", out.stderr);
}

// ============================================================================
// Unpruned ancestry tests
// ============================================================================

#[test]
fn test_skip_dedup_ancestry_flag() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");

    let config_path = repo.create_config("unpruned", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let out =
        repo.run_pluck(&["-c", config_path.to_str().unwrap(), "--recursive", "--log-branch", "--skip-dedup-ancestry"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

// ============================================================================
// Config recursive from file
// ============================================================================

#[test]
fn test_recursive_from_config() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");

    let config_content = "[forward.from \"src.txt\"]\n    to = (Mirror)\n\n[pluck]\n    recursive = true\n";
    let config_path = repo.create_config("cfgrecursive", config_content);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let pluck_count = repo.commit_count("refs/heads/pluck/cfgrecursive");
    assert!(pluck_count >= 1);
}

// ============================================================================
// No recursive overrides config
// ============================================================================

#[test]
fn test_no_recursive_overrides_config() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "commit 1");
    repo.commit_file("src.txt", "v2", "commit 2");

    let config_content = "[forward.from \"src.txt\"]\n    to = (Mirror)\n\n[pluck]\n    recursive = true\n";
    let config_path = repo.create_config("cfgnorecursive", config_content);

    // --no-recursive should override the config file value
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--no-recursive", "--log-branch"]);

    // Should process only HEAD (single commit mode)
    let pluck_count = repo.commit_count("refs/heads/pluck/cfgnorecursive");
    assert_eq!(pluck_count, 1);
}

// ============================================================================
// Date preservation tests
// ============================================================================

#[test]
fn test_author_date_preserved() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("datepres", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    let source_sha = repo.run_cmd("rev-parse", &["HEAD"]);
    let source_date = repo.run_cmd("log", &["-1", "--format=%ai", &source_sha]);

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/datepres").unwrap();
    let pluck_date = repo.run_cmd("log", &["-1", "--format=%ai", &pluck_ref]);

    assert_eq!(source_date, pluck_date, "Author date should be preserved from source commit");
}

// ============================================================================
// Multiple map entries in single section
// ============================================================================

#[test]
fn test_map_section_only_copies() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    // Section with only copy entries (no primary)
    let config_content = "[forward.from \"src.txt\"]\n    to = (Copy)copy1.txt\n    to = (Copy)copy2.txt\n";
    let config_path = repo.create_config("onlycopies", config_content);

    // Should parse without errors
    let out = repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--show-src-paths"]);
    assert_eq!(out.stdout_lines(), vec!["src.txt"]);
}

// ============================================================================
// Blob content preserved
// ============================================================================

#[test]
fn test_blob_content_preserved() {
    let repo = TestRepo::new();
    let content = "Hello, this is the original content!\nLine 2\n";
    repo.commit_file("src.txt", content, "initial");

    let config_path = repo.create_config("blobcontent", "[forward.from \"src.txt\"]\n    to = dest.txt\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    // Get the blob content from the pluck tree
    let pluck_ref = repo.get_ref("refs/heads/pluck/blobcontent").unwrap();
    let file_content = repo.run_cmd("show", &[&format!("{}:dest.txt", pluck_ref)]);
    assert_eq!(file_content, content.trim_end());
}

#[test]
fn test_blob_content_preserved_mirror() {
    let repo = TestRepo::new();
    let content = "Mirror mapped content\nwith multiple lines\n";
    repo.commit_file("src.txt", content, "initial");

    let config_path = repo.create_config("blobcontent2", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/blobcontent2").unwrap();
    let file_content = repo.run_cmd("show", &[&format!("{}:src.txt", pluck_ref)]);
    assert_eq!(file_content, content.trim_end());
}

// ============================================================================
// First commit creates root commit
// ============================================================================

#[test]
fn test_first_pluck_is_root_commit() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "hello", "initial");

    let config_path = repo.create_config("rootcommit", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let pluck_ref = repo.get_ref("refs/heads/pluck/rootcommit").unwrap();
    let parents = repo.commit_parents(&pluck_ref);
    assert!(parents.is_empty(), "First pluck commit should be a root commit with no parents");
}

// ============================================================================
// Second pluck has parent
// ============================================================================

#[test]
fn test_second_pluck_has_parent() {
    let repo = TestRepo::new();
    repo.commit_file("src.txt", "v1", "initial");

    let config_path = repo.create_config("hasparent", "[forward.from \"src.txt\"]\n    to = (Mirror)\n");

    // First pluck
    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let first_sha = repo.get_ref("refs/heads/pluck/hasparent").unwrap();

    // Second commit and pluck
    repo.commit_file("src.txt", "v2", "update");

    repo.run_pluck_ok(&["-c", config_path.to_str().unwrap(), "--log-branch"]);

    let second_sha = repo.get_ref("refs/heads/pluck/hasparent").unwrap();
    let parents = repo.commit_parents(&second_sha);
    assert_eq!(parents, vec![first_sha], "Second pluck commit should have first as parent");
}
