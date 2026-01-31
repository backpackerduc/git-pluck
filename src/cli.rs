use clap::Parser;

/// CLI arguments parsed by clap.
#[allow(clippy::struct_excessive_bools)] // each bool is a distinct CLI flag
#[derive(Parser, Debug)]
#[command(name = "git-pluck", about = "Create a plucked subset of a Git repository with full history")]
pub struct Cli {
    // -- General options --
    #[arg(short = 'c', long = "config", help = "Config file path (overrides .gitpluck/ tree resolution)")]
    pub config: Option<String>,

    #[arg(short = 'd',
        long = "debug",
        num_args = 0..=1,
        default_missing_value = "1", 
        help = "Debug output level (1=single-line, 2=multiline)")]
    pub debug: Option<Option<String>>,

    #[arg(short = 'f', long = "force", help = "Bypass sanity checks and error conditions")]
    pub force: bool,

    #[arg(long = "allow-unchanged-tree", help = "Force pluck commit even when tree is unchanged from parent")]
    pub allow_unchanged_tree: bool,

    #[arg(long = "mirror-map", help = "Replace all mappings with mirror (to = (Mirror)), strip copies")]
    pub mirror_map: bool,

    #[arg(short = 'q', long = "quiet", help = "Suppress all output")]
    pub quiet: bool,

    #[arg(short = 't', long = "timer", help = "Show per-step timing information")]
    pub timer: bool,

    // -- Pluck history options --
    #[arg(
        short = 's',
        long = "start-ref",
        default_value = "HEAD",
        help = "Start plucking from this commit (default: HEAD)"
    )]
    pub start_ref: String,

    #[arg(short = 'p', long = "allow-incomplete-ancestry", help = "Allow incomplete parent ancestry resolution")]
    pub allow_incomplete_ancestry: bool,

    #[arg(short = 'r', 
        long = "recursive", 
        num_args = 0..=1, 
        default_missing_value = "", 
        help = "Process commits recursively; optional ARGS passed to rev-list"
    )]
    pub recursive: Option<Option<String>>,

    #[arg(long = "log-branch", help = "Use log branch for incremental plucking")]
    pub log_branch: bool,

    #[arg(long = "skip-dedup-ancestry", help = "Skip transitive pluck parent deduplication")]
    pub skip_dedup_ancestry: bool,

    #[arg(long = "ignorant-pluck", help = "Add pluck commit to arbitrary branch with no regard commit history")]
    pub ignorant_pluck: Option<String>,

    // -- Pluck map options --
    #[arg(long = "add-map", help = "Add a mapping entry as SRC:DST, SRC (mirror) or SRC:+DST (copy function)")]
    pub add_map: Option<String>,

    #[arg(long = "show-src-paths", help = "Print all src-paths from the current map")]
    pub show_src_paths: bool,

    #[arg(long = "show-dst-paths", help = "Print all dst-paths from the current map")]
    pub show_dst_paths: bool,

    #[arg(long = "check-config", help = "Validate the map and print any issues")]
    pub check_config: bool,

    #[arg(long = "allow-missing-path", help = "Don't error when a mapped dst-path does not exist")]
    pub allow_missing_path: bool,

    #[arg(long = "allow-nested-map", help = "Allow nested mappings")]
    pub allow_nested_map: bool,

    #[arg(long = "remove", help = "Remove a mapping section by src-path")]
    pub remove: Option<String>,

    #[arg(long = "auto-reverse-map", help = "Auto reverse all SRC:DST path mappings")]
    pub auto_reverse_map: bool,

    // -- Pluck relationship search --
    #[arg(long = "find-source-sha", help = "Find the source SHA for a given pluck ref")]
    pub find_source_sha: Option<String>,

    #[arg(long = "find-pluck-sha", help = "Find the pluck SHA for a given source ref")]
    pub find_pluck_sha: Option<String>,

    // -- Pluck commit header replacement options --
    #[arg(long = "rep-author-name", help = "Replace the author name")]
    pub rep_author_name: Option<String>,

    #[arg(long = "rep-author-email", help = "Replace the author email")]
    pub rep_author_email: Option<String>,

    #[arg(long = "rep-author-regex", help = "Conditional author replacement via allow:deny regex")]
    pub rep_author_regex: Option<String>,

    #[arg(long = "rep-committer-name", help = "Replace the committer name")]
    pub rep_committer_name: Option<String>,

    #[arg(long = "rep-committer-email", help = "Replace the committer email")]
    pub rep_committer_email: Option<String>,

    #[arg(long = "rep-committer-regex", help = "Conditional committer replacement via allow:deny regex")]
    pub rep_committer_regex: Option<String>,

    #[arg(long = "rep-message", help = "Replace the entire commit message")]
    pub rep_message: Option<String>,

    #[arg(long = "rep-message-filter", help = "Suppress commit message lines matching regex")]
    pub rep_message_filter: Option<String>,

    #[arg(long = "log-message", help = "Add \"Plucked from: <SHA>\" to pluck commit message")]
    pub log_message: bool,

    #[arg(long = "test-mode", help = "Load all functions but exit before plucking (testing)")]
    pub test_mode: bool,

    // -- Negation flags --
    #[arg(long = "no-force", help = "Explicitly disable force")]
    pub no_force: bool,

    #[arg(long = "no-unchanged-tree", help = "Explicitly disable allow-unchanged-tree")]
    pub no_unchanged_tree: bool,

    #[arg(long = "no-mirror-map", help = "Disable mirror-map (use actual mappings)")]
    pub no_mirror_map: bool,

    #[arg(long = "no-missing-path", help = "Explicitly disable allow-missing-path")]
    pub no_missing_path: bool,

    #[arg(long = "no-nested-map", help = "Explicitly disable nested-map")]
    pub no_nested_map: bool,

    #[arg(long = "no-incomplete-ancestry", help = "Explicitly disable allow-incomplete-ancestry")]
    pub no_incomplete_ancestry: bool,

    #[arg(long = "no-auto-reverse-map", help = "Explicitly disable auto-reverse-map")]
    pub no_auto_reverse_map: bool,

    #[arg(long = "no-log-branch", help = "Explicitly disable log-branch")]
    pub no_log_branch: bool,

    #[arg(long = "no-skip-dedup-ancestry", help = "Explicitly disable skip-dedup-ancestry")]
    pub no_skip_dedup_ancestry: bool,

    #[arg(long = "no-log-message", help = "Explicitly disable log-message")]
    pub no_log_message: bool,

    #[arg(long = "no-rep-author-name", help = "Clear rep-author-name")]
    pub no_rep_author_name: bool,

    #[arg(long = "no-rep-author-email", help = "Clear rep-author-email")]
    pub no_rep_author_email: bool,

    #[arg(long = "no-rep-author-regex", help = "Clear rep-author-regex")]
    pub no_rep_author_regex: bool,

    #[arg(long = "no-rep-committer-name", help = "Clear rep-committer-name")]
    pub no_rep_committer_name: bool,

    #[arg(long = "no-rep-committer-email", help = "Clear rep-committer-email")]
    pub no_rep_committer_email: bool,

    #[arg(long = "no-rep-committer-regex", help = "Clear rep-committer-regex")]
    pub no_rep_committer_regex: bool,

    #[arg(long = "no-rep-message", help = "Clear rep-message")]
    pub no_rep_message: bool,

    #[arg(long = "no-rep-message-filter", help = "Clear rep-message-filter")]
    pub no_rep_message_filter: bool,

    #[arg(long = "no-ignorant-pluck", help = "Clear ignorant-pluck")]
    pub no_ignorant_pluck: bool,

    #[arg(long = "no-recursive", help = "Explicitly disable recursive")]
    pub no_recursive: bool,

    // pluck name name (positional)
    #[arg(help = "Pluck name (defaults to config file basename)")]
    pub pluckname: Option<String>,
}

/// Override config values with CLI flags.
///
/// Boolean flags: `--flag` or `--allow-flag` sets true, `--no-flag` sets false.
/// String flags: `--flag=VAL` sets the value, `--no-flag` clears to empty string.
/// CLI flags always take precedence over config file values.
pub fn apply_cli_to_config(cli: &Cli, config: &mut crate::config::PluckConfig) {
    if cli.force && !cli.no_force {
        config.force = true;
    } else if cli.no_force {
        config.force = false;
    }

    if let Some(Some(ref level)) = cli.debug {
        config.debug = level.parse().unwrap_or(1);
    }

    if cli.allow_unchanged_tree && !cli.no_unchanged_tree {
        config.allow_unchanged_tree = true;
    } else if cli.no_unchanged_tree {
        config.allow_unchanged_tree = false;
    }

    if cli.mirror_map && !cli.no_mirror_map {
        config.mirror_map =  true;
    } else if cli.no_mirror_map {
        config.mirror_map =  false;
    }

    if cli.quiet {
        config.quiet = true;
    }

    if cli.timer {
        config.timer = true;
    }

    if !cli.start_ref.is_empty() {
        config.start_ref.clone_from(&cli.start_ref);
    }

    if cli.allow_incomplete_ancestry && !cli.no_incomplete_ancestry {
        config.allow_incomplete_ancestry = true;
    } else if cli.no_incomplete_ancestry {
        config.allow_incomplete_ancestry = false;
    }

    if let Some(opts) = &cli.recursive
        && !cli.no_recursive
    {
        config.recursive = true;
        config.recursive_opts = opts.as_deref().and_then(|s| if s.is_empty() { None } else { Some(s.to_string()) });
    }
    if cli.no_recursive {
        config.recursive = false;
        config.recursive_opts = None;
    }

    if cli.log_branch && !cli.no_log_branch {
        config.log_branch = true;
    } else if cli.no_log_branch {
        config.log_branch = false;
    }

    if cli.skip_dedup_ancestry && !cli.no_skip_dedup_ancestry {
        config.skip_dedup_ancestry = true;
    } else if cli.no_skip_dedup_ancestry {
        config.skip_dedup_ancestry = false;
    }

    if cli.allow_missing_path && !cli.no_missing_path {
        config.allow_missing_path = true;
    } else if cli.no_missing_path {
        config.allow_missing_path = false;
    }

    if cli.allow_nested_map && !cli.no_nested_map {
        config.allow_nested_map =  true;
    } else if cli.no_nested_map {
        config.allow_nested_map =  false;
    }

    if cli.auto_reverse_map && !cli.no_auto_reverse_map {
        config.auto_reverse_map =  true;
    } else if cli.no_auto_reverse_map {
        config.auto_reverse_map =  false;
    }

    if cli.log_message && !cli.no_log_message {
        config.log_message = true;
    } else if cli.no_log_message {
        config.log_message = false;
    }

    apply_string_flag(cli.rep_author_name.as_deref(), cli.no_rep_author_name, &mut config.rep_author_name);
    apply_string_flag(cli.rep_author_email.as_deref(), cli.no_rep_author_email, &mut config.rep_author_email);
    apply_string_flag(cli.rep_author_regex.as_deref(), cli.no_rep_author_regex, &mut config.rep_author_regex);
    apply_string_flag(cli.rep_committer_name.as_deref(), cli.no_rep_committer_name, &mut config.rep_committer_name);
    apply_string_flag(cli.rep_committer_email.as_deref(), cli.no_rep_committer_email, &mut config.rep_committer_email);
    apply_string_flag(cli.rep_committer_regex.as_deref(), cli.no_rep_committer_regex, &mut config.rep_committer_regex);
    apply_string_flag(cli.rep_message.as_deref(), cli.no_rep_message, &mut config.rep_message);
    apply_string_flag(cli.rep_message_filter.as_deref(), cli.no_rep_message_filter, &mut config.rep_message_filter);

    if let Some(v) = &cli.ignorant_pluck {
        config.ignorant_pluck = if cli.no_ignorant_pluck { Some(String::new()) } else { Some(v.clone()) };
    } else if cli.no_ignorant_pluck {
        config.ignorant_pluck = Some(String::new());
    }
}

/// Apply a string CLI flag, clearing to empty string when the `--no-` variant is used.
fn apply_string_flag(cli_val: Option<&str>, no_flag: bool, target: &mut Option<String>) {
    if let Some(v) = cli_val {
        *target = if no_flag { Some(String::new()) } else { Some(v.to_string()) };
    } else if no_flag {
        *target = Some(String::new());
    }
}
