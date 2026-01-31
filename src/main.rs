mod cache;
mod cli;
mod config;
mod error;
mod log;
mod map_modify;
mod parent;
mod pluck;
mod tree;

use std::process;

use anyhow::Context;
use clap::Parser;

use crate::cli::Cli;
use crate::config::{get_pluckname, resolve_config};
use crate::error::ErrorCode;
use crate::map_modify::{add_mapping, build_pluck_map, list_destinations, list_sources, remove_mapping, validate_map};
use crate::pluck::cleanup_on_failure;

fn main() {
    let cli = Cli::parse();

    if is_test_mode() || cli.test_mode {
        process::exit(0);
    }

    let result = run(&cli);

    if let Err(ref e) = result {
        eprintln!("Error: {e}");
        let code = determine_error_code(e);
        process::exit(code.to_raw());
    }
}

/// Check if `PLUCK_TEST_MODE` is set to a non-zero, non-empty value.
fn is_test_mode() -> bool {
    std::env::var("PLUCK_TEST_MODE").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    let repo = git2::Repository::open_from_env()
        .context("Failed to open Git repository. Ensure you are in a git repo or set GIT_DIR.")?;

    let config_path_buf = cli.config.as_deref().map(std::path::PathBuf::from);
    let pluckname = cli.pluckname.clone().unwrap_or_else(|| get_pluckname(config_path_buf.as_deref()));

    let (mut config, resolved_config_path) = resolve_config(&pluckname, config_path_buf.as_deref(), &cli.start_ref)?;

    cli::apply_cli_to_config(cli, &mut config);

    // Handle subcommands that don't require plucking
    if let Some(ref add_arg) = cli.add_map {
        let path = resolved_config_path.as_ref().context("--add-map requires a config file")?;
        return add_mapping(path, add_arg, config.force);
    }

    if let Some(ref remove_arg) = cli.remove {
        let path = resolved_config_path.as_ref().context("--remove requires a config file")?;
        return remove_mapping(path, remove_arg);
    }

    if cli.show_src_paths {
        let entries = build_pluck_map(&config)?;
        for src in list_sources(&entries) {
            println!("{src}");
        }
        return Ok(());
    }

    if cli.show_dst_paths {
        let entries = build_pluck_map(&config)?;
        for dst in list_destinations(&entries) {
            println!("{dst}");
        }
        return Ok(());
    }

    if cli.check_config {
        let entries = build_pluck_map(&config)?;
        let issues = validate_map(&entries, config.allow_nested_map);
        if issues.is_empty() {
            println!("Map validation passed.");
        } else {
            for issue in &issues {
                eprintln!("Issue: {issue}");
            }
            if !config.force {
                return Err(anyhow::anyhow!("Map validation failed with {} issue(s)", issues.len()));
            }
        }
        return Ok(());
    }

    if let Some(ref query_ref) = cli.find_source_sha {
        let mut cache = crate::cache::PluckCache::new();
        cache.preload_from_log(&repo, &pluckname, &config)?;
        if let Some(sha) = cache.lookup_source_sha(query_ref) {
            println!("{sha}");
        } else {
            return Err(anyhow::anyhow!("No source found for pluck ref: {query_ref}"));
        }
        return Ok(());
    }

    if let Some(ref query_ref) = cli.find_pluck_sha {
        let mut cache = crate::cache::PluckCache::new();
        cache.preload_from_log(&repo, &pluckname, &config)?;
        if let Some(sha) = cache.lookup_pluck_sha(query_ref) {
            println!("{sha}");
        } else {
            return Err(anyhow::anyhow!("No pluck found for source ref: {query_ref}"));
        }
        return Ok(());
    }

    // Validate map
    let entries = build_pluck_map(&config)?;
    let issues = validate_map(&entries, config.allow_nested_map);
    if !issues.is_empty() && !config.force {
        return Err(anyhow::anyhow!("Map validation failed:\n{}", issues.join("\n")));
    }

    // Run the pluck
    let result = pluck::run_pluck(&repo, &pluckname, &config);

    if let Err(ref _e) = result {
        cleanup_on_failure(&repo, &pluckname);
    }

    result
}

/// Map an error to an exit code based on its message content.
fn determine_error_code(err: &anyhow::Error) -> ErrorCode {
    let msg = err.to_string().to_lowercase();

    if msg.contains("config")
        || msg.contains("conflicting")
        || msg.contains("cannot be combined")
        || msg.contains("must be enabled")
        || msg.contains("requires")
        || msg.contains("validation failed")
    {
        ErrorCode::ConfigError
    } else if msg.contains("not found in commit")
        || msg.contains("ancestry")
        || msg.contains("inconsistency")
        || msg.contains("tree generation")
    {
        ErrorCode::Internal(3)
    } else {
        ErrorCode::PluckingError
    }
}
