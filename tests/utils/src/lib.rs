use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique name for temp directories.
fn unique_name() -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    format!("pluck_test_{}_{}", std::process::id(), n)
}

/// A temporary git repository for integration tests.
///
/// Creates a repo in a unique temp directory. Automatically cleaned up on drop.
pub struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    /// Create a new temporary git repository with an initial commit.
    pub fn new() -> Self {
        let name = unique_name();
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();

        let r = TestRepo { path };
        r.run_cmd("init", &["-b", "master"]);
        r.run_cmd("config", &["user.name", "Test Author"]);
        r.run_cmd("config", &["user.email", "test@example.com"]);
        r
    }

    /// Run a git command in the repo directory.
    pub fn run_cmd(&self, cmd: &str, args: &[&str]) -> String {
        let output =
            Command::new("git").arg(cmd).args(args).current_dir(&self.path).output().expect("Failed to run git");
        if !output.status.success() {
            panic!("git {} {:?} failed: {}", cmd, args, String::from_utf8_lossy(&output.stderr));
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Create a file and commit it with the given message.
    pub fn commit_file(&self, filename: &str, content: &str, message: &str) -> String {
        let file_path = self.path.join(filename);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&file_path, content).unwrap();
        self.run_cmd("add", &[filename]);
        self.run_cmd("commit", &["-m", message]);
        self.run_cmd("rev-parse", &["HEAD"])
    }

    /// Run the git-pluck binary with the given args and return stdout.
    pub fn run_pluck(&self, args: &[&str]) -> CommandOutput {
        let binary = std::env::var("CARGO_BIN_EXE_git-pluck").expect("CARGO_BIN_EXE_git-pluck not set");
        let output =
            Command::new(&binary).args(args).current_dir(&self.path).output().expect("Failed to run git-pluck");
        CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            code: output.status.code().unwrap_or(-1),
        }
    }

    /// Run the git-pluck binary and assert it succeeded (exit 0).
    pub fn run_pluck_ok(&self, args: &[&str]) -> CommandOutput {
        let out = self.run_pluck(args);
        assert_eq!(out.code, 0, "git-pluck failed with exit code {}: stderr: {}", out.code, out.stderr);
        out
    }

    /// Get the SHA of a ref, or None if the ref does not exist.
    pub fn get_ref(&self, name: &str) -> Option<String> {
        let output =
            Command::new("git").args(["rev-parse", "--verify", name]).current_dir(&self.path).output().unwrap();
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() { None } else { Some(stdout) }
    }

    /// List files in a tree (commit SHA or ref).
    pub fn ls_tree(&self, rev: &str) -> Vec<String> {
        let output =
            Command::new("git").args(["ls-tree", "-r", "--name-only", rev]).current_dir(&self.path).output().unwrap();
        String::from_utf8_lossy(&output.stdout).lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect()
    }

    /// Get the commit message for a ref.
    pub fn commit_message(&self, rev: &str) -> String {
        self.run_cmd("log", &["-1", "--format=%B", rev])
    }

    /// Get author info for a commit.
    pub fn commit_author(&self, rev: &str) -> (String, String) {
        let name = self.run_cmd("log", &["-1", "--format=%an", rev]);
        let email = self.run_cmd("log", &["-1", "--format=%ae", rev]);
        (name, email)
    }

    /// Get committer info for a commit.
    pub fn commit_committer(&self, rev: &str) -> (String, String) {
        let name = self.run_cmd("log", &["-1", "--format=%cn", rev]);
        let email = self.run_cmd("log", &["-1", "--format=%ce", rev]);
        (name, email)
    }

    /// Create a merge commit.
    pub fn merge_commit(&self, branch: &str, message: &str) -> String {
        self.run_cmd("checkout", &["-b", branch]);
        self.commit_file(&format!("merge_file_{}.txt", branch), "merge", "merge branch commit");
        self.run_cmd("checkout", &["master"]);
        self.run_cmd("merge", &["--no-ff", "-m", message, branch]);
        self.run_cmd("rev-parse", &["HEAD"])
    }

    /// Create a config file at `.gitpluck/<pluckname>.pluck`.
    pub fn create_config(&self, pluckname: &str, content: &str) -> PathBuf {
        let config_dir = self.path.join(".gitpluck");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join(format!("{pluckname}.pluck"));
        fs::write(&config_path, content).unwrap();
        config_path
    }

    /// Get the parent SHAs of a commit.
    pub fn commit_parents(&self, rev: &str) -> Vec<String> {
        let parents = self.run_cmd("log", &["-1", "--format=%P", rev]);
        if parents.is_empty() { Vec::new() } else { parents.split_whitespace().map(|s| s.to_string()).collect() }
    }

    /// Count the number of commits in a ref.
    pub fn commit_count(&self, rev: &str) -> usize {
        let count = self.run_cmd("rev-list", &["--count", rev]);
        count.parse().unwrap_or(0)
    }

    /// Checkout a branch or commit.
    pub fn checkout(&self, name: &str) {
        self.run_cmd("checkout", &[name]);
    }

    /// Create a new branch starting from a specific commit/ref.
    pub fn create_branch(&self, name: &str, start_point: &str) {
        self.run_cmd("branch", &[name, start_point]);
    }

    /// Merge a branch into the current branch with --no-ff.
    pub fn merge(&self, branch: &str, message: &str) -> String {
        self.run_cmd("merge", &["--no-ff", "-m", message, branch]);
        self.run_cmd("rev-parse", &["HEAD"])
    }

    /// Get commit messages for all commits reachable from a ref (oldest first).
    pub fn commit_messages(&self, rev: &str) -> Vec<String> {
        let output = Command::new("git")
            .args(["log", "--format=%s", "--topo-order", "--reverse", rev])
            .current_dir(&self.path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).lines().map(|l| l.to_string()).collect()
    }

    /// Check if a commit SHA is reachable from a ref.
    pub fn is_reachable(&self, sha: &str, from: &str) -> bool {
        let output = Command::new("git")
            .args(["merge-base", "--is-ancestor", sha, from])
            .current_dir(&self.path)
            .output()
            .unwrap();
        output.status.success()
    }

    /// Run a git command with extra environment variables set.
    pub fn run_cmd_with_env(&self, cmd: &str, args: &[&str], env: &[(&str, &str)]) -> String {
        let output = Command::new("git")
            .arg(cmd)
            .args(args)
            .current_dir(&self.path)
            .envs(env.iter().map(|(k, v)| (*k, *v)))
            .output()
            .unwrap();
        if !output.status.success() {
            panic!("git {} {:?} failed: {}", cmd, args, String::from_utf8_lossy(&output.stderr));
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// List all commits (SHAs) on a ref, oldest first.
    pub fn commit_shas(&self, rev: &str) -> Vec<String> {
        self.run_cmd("rev-list", &["--reverse", rev]).lines().map(|s| s.to_string()).collect()
    }

    /// Write a file, creating parent directories if needed.
    pub fn write_file(&self, filename: &str, content: &str) {
        let file_path = self.path.join(filename);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&file_path, content).unwrap();
    }

    /// Get the path to the temp repo directory.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl CommandOutput {
    pub fn stdout_lines(&self) -> Vec<String> {
        self.stdout.lines().map(|s| s.to_string()).collect()
    }
}
