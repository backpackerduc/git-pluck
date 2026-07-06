use std::path::{Path, PathBuf};

use anyhow::{Ok, bail};

/// Pluck behavior configuration.
///
/// Holds all settings with their defaults. CLI flags override these values.
#[allow(clippy::struct_excessive_bools)] // each bool is a distinct config option
#[derive(Debug, Clone)]
pub struct PluckConfig {
    pub debug: u8,
    pub force: bool,
    pub allow_unchanged_tree: bool,
    pub mirror_map: bool,
    pub start_ref: String,
    pub allow_missing_path: bool,
    pub rep_author_regex: Option<String>,
    pub rep_author_name: Option<String>,
    pub rep_author_email: Option<String>,
    pub rep_committer_regex: Option<String>,
    pub rep_committer_name: Option<String>,
    pub rep_committer_email: Option<String>,
    pub rep_message: Option<String>,
    pub rep_message_filter: Option<String>,
    pub allow_nested_map: bool,
    pub allow_incomplete_ancestry: bool,
    pub quiet: bool,
    pub recursive: bool,
    pub recursive_opts: Option<String>,
    pub auto_reverse_map: bool,
    pub log_message: bool,
    pub timer: bool,
    pub log_branch: bool,
    pub skip_dedup_ancestry: bool,
    pub ignorant_pluck: Option<String>,
    pub path_mappings: Vec<PathMappingEntry>,
}

/// A raw `[forward.from "<source>"]` section entry with its `map` values.
#[derive(Debug, Clone)]
pub struct PathMappingEntry {
    pub src_path: String,
    pub dst_paths: Vec<String>,
}

impl Default for PluckConfig {
    fn default() -> Self {
        Self {
            debug: 0,
            force: false,
            allow_unchanged_tree: false,
            mirror_map: false,
            start_ref: "HEAD".to_string(),
            allow_missing_path: false,
            rep_author_regex: None,
            rep_author_name: None,
            rep_author_email: None,
            rep_committer_regex: None,
            rep_committer_name: None,
            rep_committer_email: None,
            rep_message: None,
            rep_message_filter: None,
            allow_nested_map: false,
            allow_incomplete_ancestry: false,
            quiet: false,
            recursive: false,
            recursive_opts: None,
            auto_reverse_map: false,
            log_message: false,
            timer: false,
            log_branch: true,
            skip_dedup_ancestry: false,
            ignorant_pluck: None,
            path_mappings: Vec::new(),
        }
    }
}

/// Resolve and parse the config for a pluck name.
///
/// Priority: explicit `-c` file > embedded at source start reference > working tree.
/// Returns the parsed config and the resolved config file path (if any).
pub fn resolve_config(
    pluckname: &str,
    explicit_config: Option<&Path>,
    start_ref: &str,
) -> anyhow::Result<(PluckConfig, Option<PathBuf>)> {
    let config_path = if let Some(path) = explicit_config {
        anyhow::ensure!(path.exists(), "Config file not found: {}", path.display());
        anyhow::ensure!(path.is_file(), "Config path is not a regular file: {}", path.display());
        Some(path.to_path_buf())
    } else {
        resolve_embedded_config(pluckname, start_ref).or_else(|| resolve_working_tree_config(pluckname))
    };

    let mut config = PluckConfig::default();

    if let Some(ref path) = config_path {
        parse_config_file(path, &mut config)?;
    }

    if config_path.is_none() && !config.force {
        bail!("No config found for pluck name '{pluckname}'. Use -c to specify a config file.");
    }

    Ok((config, config_path))
}

/// Try to find a config file embedded in the start reference tree at `.gitpluck/<pluckname>.pluck`.
fn resolve_embedded_config(pluckname: &str, start_ref: &str) -> Option<PathBuf> {
    let repo = git2::Repository::open_from_env().ok()?;
    let commit = repo.revparse_single(start_ref).ok()?.into_commit().ok()?;
    let tree = commit.tree().ok()?;
    let config_path = format!(".gitpluck/{pluckname}.pluck");
    tree.get_path(&PathBuf::from(&config_path)).ok()?.to_object(&repo).ok()?;
    Some(PathBuf::from(&config_path))
}

/// Try to find a config file at `.gitpluck/<pluckname>.pluck` in the working directory.
fn resolve_working_tree_config(pluckname: &str) -> Option<PathBuf> {
    let repo = git2::Repository::open_from_env().ok()?;
    let workdir = repo.workdir()?.to_path_buf();
    let config_path = workdir.join(".gitpluck").join(format!("{pluckname}.pluck"));
    if config_path.exists() { Some(config_path) } else { None }
}

/// Parse a Git INI config file into a `PluckConfig`.
///
/// Handles `[forward.from "<path>"]` sections with `map` keys and `[pluck]` section
/// with behavior settings. Uses `gix-config` for proper git INI parsing.
fn parse_config_file(path: &Path, config: &mut PluckConfig) -> anyhow::Result<()> {
    let file = gix_config::File::from_path_no_includes(path.to_path_buf(), gix_config::Source::Api)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file: {e}"))?;

    for section in file.sections() {
        let header = section.header();
        let section_name = header.name();

        if section_name == b"forward.from" {
            let Some(subsection) = header.subsection_name() else { continue };
            let src_path = to_string(subsection);
            let dst_paths = section.body().values("to").into_iter().map(|v| to_string(&v)).collect();
            config.path_mappings.push(PathMappingEntry { src_path, dst_paths });
        } else if section_name == b"pluck" {
            parse_pluck_section(section, config)?;
        }
    }

    Ok(())
}

/// Parse all keys from the `[pluck]` section.
fn parse_pluck_section(section: &gix_config::file::Section<'_>, config: &mut PluckConfig) -> anyhow::Result<()> {
    let body = section.body();

    // Use gix-config's body iteration to get all key-value pairs.
    // The Body iterator yields (ValueName, Cow<BStr>) but only the last value per key.
    // For single-value keys, body.value() is sufficient.
    for (key_name, value) in body.clone().into_iter() {
        let key = key_name.as_ref().trim().to_string();
        let val = to_string(&value);
        set_pluck_config(&key, &val, config)?;
    }

    Ok(())
}

/// Set a single pluck behavior key from a parsed config value.
fn set_pluck_config(key: &str, value: &str, config: &mut PluckConfig) -> anyhow::Result<()> {
    match key {
        "debug" => config.debug = value.parse().unwrap_or(0),
        "force" => config.force = parse_bool(value)?,
        "allowUnchangedTree" => config.allow_unchanged_tree = parse_bool(value)?,
        "mirrorMap" => config.mirror_map = parse_bool(value)?,
        "startRef" => {
            ensure_not_empty("startRef", value)?;
            config.start_ref = value.to_string();
        }
        "allowMissingPath" => config.allow_missing_path = parse_bool(value)?,
        "repAuthorRegex" => {
            ensure_not_empty("repAuthorRegex", value)?;
            config.rep_author_regex = Some(value.to_string());
        }
        "repAuthorName" => {
            ensure_not_empty("repAuthorName", value)?;
            config.rep_author_name = Some(value.to_string());
        }
        "repAuthorEmail" => {
            ensure_not_empty("repAuthorEmail", value)?;
            config.rep_author_email = Some(value.to_string());
        }
        "repCommitterRegex" => {
            ensure_not_empty("repCommitterRegex", value)?;
            config.rep_committer_regex = Some(value.to_string());
        }
        "repCommitterName" => {
            ensure_not_empty("repCommitterName", value)?;
            config.rep_committer_name = Some(value.to_string());
        }
        "repCommitterEmail" => {
            ensure_not_empty("repCommitterEmail", value)?;
            config.rep_committer_email = Some(value.to_string());
        }
        "repMessage" => {
            ensure_not_empty("repMessage", value)?;
            config.rep_message = Some(value.to_string());
        }
        "repMessageFilter" => {
            ensure_not_empty("repMessageFilter", value)?;
            config.rep_message_filter = Some(value.to_string());
        }
        "allowNestedMap" => config.allow_nested_map = parse_bool(value)?,
        "allowIncompleteAncestry" => config.allow_incomplete_ancestry = parse_bool(value)?,
        "quiet" => config.quiet = parse_bool(value)?,
        "recursive" => {
            config.recursive = !value.is_empty();
            config.recursive_opts = if value.is_empty() { None } else { Some(value.to_string()) };
        }
        "autoReverseMap" => config.auto_reverse_map = parse_bool(value)?,
        "logMessage" => config.log_message = parse_bool(value)?,
        "timer" => config.timer = parse_bool(value)?,
        "logBranch" => config.log_branch = parse_bool(value)?,
        "unprunedAncestry" => config.skip_dedup_ancestry = parse_bool(value)?,
        "ignorantPluck" => {
            ensure_not_empty("ignorantPluck", value)?;
            config.ignorant_pluck = Some(value.to_string());
        }
        _ => eprintln!("Warning: unknown config key '{key}', ignoring!"),
    }

    Ok(())
}

fn to_string(b: &bstr::BStr) -> String {
    String::from_utf8_lossy(b).trim().to_string()
}

fn ensure_not_empty(key: &str, value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!value.is_empty(), "Config key '{key}' must not be empty. Remove or set value!");
    Ok(())
}

/// Parse a boolean config value. Accepts `true`/`false`/`1`/`0`.
/// Returns an error for empty or unrecognized values.
fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => anyhow::bail!("Invalid boolean value: '{value}' (expected true/false or 1/0)"),
    }
}

/// Derive a Pluck name from a config file path.
///
/// Uses the file stem (basename without extension). Falls back to "default".
pub fn get_pluckname(config_path: Option<&Path>) -> String {
    if let Some(path) = config_path {
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("default").to_string()
    } else {
        "default".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let config = PluckConfig::default();
        assert_eq!(config.debug, 0);
        assert!(!config.force);
        assert!(!config.allow_unchanged_tree);
        assert!(!config.mirror_map);
        assert_eq!(config.start_ref, "HEAD");
        assert!(!config.allow_missing_path);
        assert!(config.rep_author_regex.is_none());
        assert!(config.rep_author_name.is_none());
        assert!(config.rep_author_email.is_none());
        assert!(config.rep_committer_regex.is_none());
        assert!(config.rep_committer_name.is_none());
        assert!(config.rep_committer_email.is_none());
        assert!(config.rep_message.is_none());
        assert!(config.rep_message_filter.is_none());
        assert!(!config.allow_nested_map);
        assert!(!config.allow_incomplete_ancestry);
        assert!(!config.quiet);
        assert!(!config.recursive);
        assert!(config.recursive_opts.is_none());
        assert!(!config.auto_reverse_map);
        assert!(!config.log_message);
        assert!(!config.timer);
        assert!(config.log_branch);
        assert!(!config.skip_dedup_ancestry);
        assert!(config.ignorant_pluck.is_none());
        assert!(config.path_mappings.is_empty());
    }

    #[test]
    fn test_parse_bool_true() {
        assert!(parse_bool("true").unwrap());
    }

    #[test]
    fn test_parse_bool_false() {
        assert!(!parse_bool("false").unwrap());
    }

    #[test]
    fn test_parse_bool_one() {
        assert!(parse_bool("1").unwrap());
    }

    #[test]
    fn test_parse_bool_zero() {
        assert!(!parse_bool("0").unwrap());
    }

    #[test]
    fn test_parse_bool_invalid_errors() {
        assert!(parse_bool("yes").is_err());
        assert!(parse_bool("").is_err());
        assert!(parse_bool("random").is_err());
    }

    #[test]
    fn test_get_pluckname_src_path() {
        let path = Path::new("/some/path/mypluck");
        assert_eq!(get_pluckname(Some(path)), "mypluck");
    }

    #[test]
    fn test_get_pluckname_src_path_with_extension() {
        let path = Path::new("/some/path/mypluck.pluck");
        assert_eq!(get_pluckname(Some(path)), "mypluck");
    }

    #[test]
    fn test_get_pluckname_none() {
        assert_eq!(get_pluckname(None), "default");
    }

    #[test]
    fn test_get_pluckname_no_stem() {
        let path = Path::new("/");
        assert_eq!(get_pluckname(Some(path)), "default");
    }

    #[test]
    fn test_parse_config_path_mappings() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_config_{}", std::process::id()));
        let content = r#"
[forward.from "src"]
    to = dest
[forward.from "docs"]
    to = (Mirror)
[forward.from "vendor"]
    to = (Remove)
"#;
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(config.path_mappings.len(), 3);
        assert_eq!(config.path_mappings[0].src_path, "src");
        assert_eq!(config.path_mappings[0].dst_paths, vec!["dest"]);
        assert_eq!(config.path_mappings[1].src_path, "docs");
        assert_eq!(config.path_mappings[1].dst_paths, vec!["(Mirror)"]);
        assert_eq!(config.path_mappings[2].src_path, "vendor");
        assert_eq!(config.path_mappings[2].dst_paths, vec!["(Remove)"]);
    }

    #[test]
    fn test_parse_config_multiple_maps_same_section() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_multi_map_{}", std::process::id()));
        let content = r#"
[forward.from "src"]
    to = dest
    to = (Copy)copy1
    to = (Copy)copy2
"#;
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(config.path_mappings.len(), 1);
        assert_eq!(config.path_mappings[0].src_path, "src");
        assert_eq!(config.path_mappings[0].dst_paths, vec!["dest", "(Copy)copy1", "(Copy)copy2"]);
    }

    #[test]
    fn test_parse_config_behavior_keys() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_behavior_{}", std::process::id()));
        let content = r#"
[forward.from "src"]
    to = (Mirror)

[pluck]
    force = true
    debug = 2
    recursive = max-count:10
    startRef = abc123
    logBranch = false
"#;
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(config.force);
        assert_eq!(config.debug, 2);
        assert!(config.recursive);
        assert_eq!(config.recursive_opts, Some("max-count:10".to_string()));
        assert_eq!(config.start_ref, "abc123");
        assert!(!config.log_branch);
    }

    #[test]
    fn test_parse_config_comments_ignored() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_comments_{}", std::process::id()));
        let content = r#"
# This is a comment
; This is also a comment
[forward.from "src"]
    to = (Mirror)
"#;
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(config.path_mappings.len(), 1);
    }

    #[test]
    fn test_parse_config_empty_recursive() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_empty_rec_{}", std::process::id()));
        let content = "[pluck]\n    recursive = true\n";
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(config.recursive);
        assert_eq!(config.recursive_opts, Some("true".to_string()));
    }

    #[test]
    fn test_parse_config_truly_empty_recursive() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_empty_rec2_{}", std::process::id()));
        let content = "[pluck]\n    recursive = \n";
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(!config.recursive);
        assert!(config.recursive_opts.is_none());
    }

    #[test]
    fn test_set_pluck_config_unknown_key_ignored() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_unknown_{}", std::process::id()));
        let content = "[pluck]\n    unknownKey = value\n    anotherUnknown = 123\n";
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(config.path_mappings.len(), 0);
    }

    // -- config parsing with inline comments --

    #[test]
    fn test_parse_config_map_with_inline_comment() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_inline_{}", std::process::id()));
        let content = r#"
[forward.from "lib"]
    to = libs/1   # maps "lib" to "libs/1"
[forward.from "lib2"]
    to = libs/2   # maps "lib2" to "libs/2"
"#;
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(config.path_mappings.len(), 2);
        assert_eq!(config.path_mappings[0].dst_paths, vec!["libs/1"]);
        assert_eq!(config.path_mappings[1].dst_paths, vec!["libs/2"]);
    }

    #[test]
    fn test_parse_config_pluck_section_with_inline_comment() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_inline2_{}", std::process::id()));
        let content = "[pluck]\n    force = true   # enable force mode\n";
        std::fs::write(&tmp, content).unwrap();
        let mut config = PluckConfig::default();
        parse_config_file(&tmp, &mut config).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(config.force);
    }
}
