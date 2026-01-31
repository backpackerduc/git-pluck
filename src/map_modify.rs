use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::bail;

use crate::config::PluckConfig;

/// A single resolved entry in the pluck map.
#[derive(Debug, Clone)]
pub struct MapEntry {
    pub src_path: String,
    pub primary: MapTarget,
    pub copies: Vec<String>,
}

/// The destination type for a map entry.
#[derive(Debug, Clone)]
pub enum MapTarget {
    Mirror,
    Remove,
    Root,
    Move(String),
}

/// Build the resolved pluck map from config tree mappings.
///
/// When `mirror_map` is set, all mappings become mirror (`to = (Mirror)`)
/// and copies are stripped. Otherwise applies `auto_reverse_map` when enabled.
pub fn build_pluck_map(config: &PluckConfig) -> anyhow::Result<Vec<MapEntry>> {
    if config.mirror_map {
        return Ok(config
            .path_mappings
            .iter()
            .map(|e| MapEntry { src_path: e.src_path.clone(), primary: MapTarget::Mirror, copies: Vec::new() })
            .collect());
    }

    let mut entries: Vec<MapEntry> = Vec::new();

    for entry in &config.path_mappings {
        let mut primary: Option<MapTarget> = None;
        let mut copies: Vec<String> = Vec::new();

        for map_val in &entry.dst_paths {
            if let Some(copy_path) = map_val.strip_prefix("(Copy)") {
                copies.push(copy_path.to_string());
            } else if primary.is_none() {
                primary = Some(parse_map_target(map_val));
            }
        }

        entries.push(MapEntry {
            src_path: entry.src_path.clone(),
            primary: primary.unwrap_or(MapTarget::Mirror),
            copies,
        });
    }

    if config.auto_reverse_map {
        auto_reverse_map(&mut entries);
    }

    Ok(entries)
}

/// Parse a raw `to = <value>` string into a `MapTarget`.
fn parse_map_target(value: &str) -> MapTarget {
    match value {
        "(Mirror)" => MapTarget::Mirror,
        "(Remove)" => MapTarget::Remove,
        "." => MapTarget::Root,
        _ => MapTarget::Move(value.to_string()),
    }
}

/// Swap source and destination for all non-remove entries.
fn auto_reverse_map(entries: &mut [MapEntry]) {
    for entry in entries {
        if matches!(&entry.primary, MapTarget::Remove) {
            continue;
        }
        if let MapTarget::Move(ref dst) = entry.primary {
            let src = std::mem::take(&mut entry.src_path);
            entry.src_path = dst.clone();
            entry.primary = MapTarget::Move(src);
        }
    }
}

/// Validate the map for duplicates, trailing slashes, nesting, and colons.
///
/// Returns a list of issue descriptions. Empty means the map is valid.
pub fn validate_map(entries: &[MapEntry], allow_nested: bool) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    let mut seen_dst: HashMap<String, String> = HashMap::new();

    for entry in entries {
        let src = &entry.src_path;

        if src.ends_with('/') {
            issues.push(format!("Trailing slash in source: {src}"));
        }

        if src.contains(':') {
            issues.push(format!("Erroneous colon in source: {src}"));
        }

        let dst = match &entry.primary {
            MapTarget::Mirror => src.clone(),
            MapTarget::Remove => String::new(),
            MapTarget::Root => ".".to_string(),
            MapTarget::Move(d) => d.clone(),
        };

        if !dst.is_empty() && dst.ends_with('/') {
            issues.push(format!("Trailing slash in destination: {dst}"));
        }
        if !dst.is_empty() && dst.contains(':') {
            issues.push(format!("Erroneous colon in destination: {dst}"));
        }

        if !dst.is_empty() && !matches!(&entry.primary, MapTarget::Remove) {
            if let Some(existing) = seen_dst.get(&dst) {
                issues.push(format!("Duplicate destination '{dst}': mapped from '{existing}' and '{src}'"));
            }
            seen_dst.insert(dst, src.clone());
        }
    }

    if !allow_nested {
        check_nested(entries, &mut issues);
    }

    issues
}

/// Check for nested source paths (e.g., `a/b` and `a/b/c`).
fn check_nested(entries: &[MapEntry], issues: &mut Vec<String>) {
    let sources: Vec<&str> = entries.iter().map(|e| e.src_path.as_str()).collect();
    for i in 0..sources.len() {
        for j in (i + 1)..sources.len() {
            let a = sources[i];
            let b = sources[j];
            if a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/")) {
                let (inner, outer) = if a.len() < b.len() { (a, b) } else { (b, a) };
                issues.push(format!("Nested tree mapping: '{inner}' is a subpath of '{outer}'"));
            }
        }
    }
}

/// Add a `src:dst` mapping to the config file.
///
/// If the argument has no colon, creates an mirror map (`to = (Mirror)`).
/// If `dst` is the literal string `false`, creates a remove (`to = (Remove)`).
/// Returns an error if the source already exists with mirror-map form and `--force` is not set.
pub fn add_mapping(config_path: &Path, src_dst: &str, force: bool) -> anyhow::Result<()> {
    let content = fs::read_to_string(config_path)?;

    let (src, dst) = if let Some(colon_pos) = src_dst.find(":") {
        (&src_dst[..colon_pos], Some(&src_dst[colon_pos + 1..]))
    } else {
        (src_dst, None)
    };

    let has_section = content.lines().any(|line| {
        let t = line.trim();
        t == format!("[forward.from \"{src}\"]") || t == format!("[forward.from {src}]")
    });

    if has_section && dst.is_none() && !force {
        bail!("Source '{src}' already exists in config. Use --force to override.");
    }

    let mut lines: Vec<String> = content.lines().map(std::string::ToString::to_string).collect();

    if has_section {
        let mut in_section = false;
        let mut section_end = lines.len();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == format!("[forward.from \"{src}\"]") || t == format!("[forward.from {src}]") {
                in_section = true;
                continue;
            }
            if in_section && t.starts_with('[') {
                section_end = i;
                break;
            }
        }
        let new_map = if let Some(d) = dst { format!("    to = {d}") } else { "    to = (Mirror)".to_string() };
        lines.splice(section_end..section_end, vec![new_map]);
    } else {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[forward.from \"{src}\"]"));
        let map_val = dst.unwrap_or("(Mirror)");
        lines.push(format!("    to = {map_val}"));
    }

    fs::write(config_path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Remove a `[forward.from "<src>"]` section from the config file.
pub fn remove_mapping(config_path: &Path, src: &str) -> anyhow::Result<()> {
    let content = fs::read_to_string(config_path)?;

    let mut result = Vec::new();
    let mut skip = false;
    let mut found = false;

    for line in content.lines() {
        let t = line.trim();
        if t == format!("[forward.from \"{src}\"]") || t == format!("[forward.from {src}]") {
            skip = true;
            found = true;
            continue;
        }
        if skip {
            if t.starts_with('[') {
                skip = false;
                result.push(line.to_string());
            }
            continue;
        }
        result.push(line.to_string());
    }

    if !found {
        bail!("Source '{src}' not found in config.");
    }

    fs::write(config_path, result.join("\n") + "\n")?;
    Ok(())
}

/// Return all source paths from the resolved map.
pub fn list_sources(entries: &[MapEntry]) -> Vec<String> {
    entries.iter().map(|e| e.src_path.clone()).collect()
}

/// Return all non-removed destination paths from the resolved map.
pub fn list_destinations(entries: &[MapEntry]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|e| match &e.primary {
            MapTarget::Mirror => Some(e.src_path.clone()),
            MapTarget::Remove => None,
            MapTarget::Root => Some(".".to_string()),
            MapTarget::Move(d) => Some(d.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(src_path: &str, primary: MapTarget, copies: Vec<String>) -> MapEntry {
        MapEntry { src_path: src_path.to_string(), primary, copies }
    }

    // -- parse_map_target --

    #[test]
    fn test_parse_map_target_mirror() {
        assert!(matches!(parse_map_target("(Mirror)"), MapTarget::Mirror));
    }

    #[test]
    fn test_parse_map_target_remove() {
        assert!(matches!(parse_map_target("(Remove)"), MapTarget::Remove));
    }

    #[test]
    fn test_parse_map_target_unpack() {
        assert!(matches!(parse_map_target("."), MapTarget::Root));
    }

    #[test]
    fn test_parse_map_target_move() {
        let target = parse_map_target("some/path");
        assert!(matches!(&target, MapTarget::Move(p) if p == "some/path"));
    }

    // -- build_pluck_map --

    #[test]
    fn test_build_pluck_map_empty() {
        let config = crate::config::PluckConfig::default();
        let entries = build_pluck_map(&config).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_build_pluck_map_mirror() {
        let mut config = crate::config::PluckConfig::default();
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "src".to_string(),
            dst_paths: vec!["(Mirror)".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].primary, MapTarget::Mirror));
    }

    #[test]
    fn test_build_pluck_map_remove() {
        let mut config = crate::config::PluckConfig::default();
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "vendor".to_string(),
            dst_paths: vec!["(Remove)".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].primary, MapTarget::Remove));
    }

    #[test]
    fn test_build_pluck_map_move() {
        let mut config = crate::config::PluckConfig::default();
        config
            .path_mappings
            .push(crate::config::PathMappingEntry { src_path: "src".to_string(), dst_paths: vec!["dest".to_string()] });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0].primary, MapTarget::Move(d) if d == "dest"));
    }

    #[test]
    fn test_build_pluck_map_unpack() {
        let mut config = crate::config::PluckConfig::default();
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "src/core".to_string(),
            dst_paths: vec![".".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].primary, MapTarget::Root));
    }

    #[test]
    fn test_build_pluck_map_copy_entries() {
        let mut config = crate::config::PluckConfig::default();
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "src".to_string(),
            dst_paths: vec!["dest".to_string(), "(Copy)copy1".to_string(), "(Copy)copy2".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0].primary, MapTarget::Move(d) if d == "dest"));
        assert_eq!(entries[0].copies, vec!["copy1", "copy2"]);
    }

    #[test]
    fn test_build_pluck_map_only_copies() {
        let mut config = crate::config::PluckConfig::default();
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "src".to_string(),
            dst_paths: vec!["(Copy)copy1".to_string(), "(Copy)copy2".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        // No primary -> defaults to Mirror
        assert!(matches!(entries[0].primary, MapTarget::Mirror));
        assert_eq!(entries[0].copies, vec!["copy1", "copy2"]);
    }

    // -- auto_reverse_map --

    #[test]
    fn test_build_pluck_map_reverse() {
        let mut config = crate::config::PluckConfig::default();
        config.auto_reverse_map = true;
        config
            .path_mappings
            .push(crate::config::PathMappingEntry { src_path: "src".to_string(), dst_paths: vec!["dest".to_string()] });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        // Source and dest should be swapped
        assert_eq!(entries[0].src_path, "dest");
        assert!(matches!(&entries[0].primary, MapTarget::Move(s) if s == "src"));
    }

    #[test]
    fn test_build_pluck_map_reverse_remove_unchanged() {
        let mut config = crate::config::PluckConfig::default();
        config.auto_reverse_map = true;
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "vendor".to_string(),
            dst_paths: vec!["(Remove)".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].src_path, "vendor");
        assert!(matches!(entries[0].primary, MapTarget::Remove));
    }

    // -- mirror_map --

    #[test]
    fn test_build_pluck_map_mirror_map() {
        let mut config = crate::config::PluckConfig::default();
        config.mirror_map = true;
        config
            .path_mappings
            .push(crate::config::PathMappingEntry { src_path: "src".to_string(), dst_paths: vec!["dest".to_string()] });
        let entries = build_pluck_map(&config).unwrap();
        // mirror_map replaces all mappings with map=true, strips copies
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].src_path, "src");
        assert!(matches!(entries[0].primary, MapTarget::Mirror));
        assert!(entries[0].copies.is_empty());
    }

    #[test]
    fn test_build_pluck_map_mirror_map_strips_copies() {
        let mut config = crate::config::PluckConfig::default();
        config.mirror_map = true;
        config.path_mappings.push(crate::config::PathMappingEntry {
            src_path: "src".to_string(),
            dst_paths: vec!["dest".to_string(), "(Copy)copy1".to_string(), "(Copy)copy2".to_string()],
        });
        let entries = build_pluck_map(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].primary, MapTarget::Mirror));
        assert!(entries[0].copies.is_empty());
    }

    // -- validate_map --

    #[test]
    fn test_validate_map_clean() {
        let entries = vec![make_entry("src", MapTarget::Move("dest".to_string()), vec![])];
        let issues = validate_map(&entries, false);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_map_duplicate_destination() {
        let entries = vec![
            make_entry("a", MapTarget::Move("same".to_string()), vec![]),
            make_entry("b", MapTarget::Move("same".to_string()), vec![]),
        ];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("Duplicate")));
    }

    #[test]
    fn test_validate_map_trailing_slash_source() {
        let entries = vec![make_entry("src/", MapTarget::Mirror, vec![])];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("Trailing slash")));
    }

    #[test]
    fn test_validate_map_trailing_slash_destination() {
        let entries = vec![make_entry("src", MapTarget::Move("dest/".to_string()), vec![])];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("Trailing slash")));
    }

    #[test]
    fn test_validate_map_nested_error() {
        let entries = vec![make_entry("a", MapTarget::Mirror, vec![]), make_entry("a/b", MapTarget::Mirror, vec![])];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("Nested")));
    }

    #[test]
    fn test_validate_map_nested_allowed() {
        let entries = vec![make_entry("a", MapTarget::Mirror, vec![]), make_entry("a/b", MapTarget::Mirror, vec![])];
        let issues = validate_map(&entries, true);
        assert!(issues.iter().all(|i| !i.contains("Nested")));
    }

    #[test]
    fn test_validate_map_colon_in_source() {
        let entries = vec![make_entry("src:bad", MapTarget::Mirror, vec![])];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("colon")));
    }

    #[test]
    fn test_validate_map_colon_in_destination() {
        let entries = vec![make_entry("src", MapTarget::Move("dst:bad".to_string()), vec![])];
        let issues = validate_map(&entries, false);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("colon")));
    }

    #[test]
    fn test_validate_map_mirror_no_duplicate() {
        // Mirror maps use source as destination, so two different sources should be fine
        let entries = vec![make_entry("a", MapTarget::Mirror, vec![]), make_entry("b", MapTarget::Mirror, vec![])];
        let issues = validate_map(&entries, false);
        assert!(issues.iter().all(|i| !i.contains("Duplicate")));
    }

    #[test]
    fn test_validate_map_remove_no_duplicate_check() {
        // Remove entries should not trigger duplicate destination checks
        let entries = vec![make_entry("a", MapTarget::Remove, vec![]), make_entry("b", MapTarget::Remove, vec![])];
        let issues = validate_map(&entries, false);
        assert!(issues.iter().all(|i| !i.contains("Duplicate")));
    }

    // -- list_sources / list_destinations --

    #[test]
    fn test_list_sources() {
        let entries =
            vec![make_entry("a", MapTarget::Move("x".to_string()), vec![]), make_entry("b", MapTarget::Mirror, vec![])];
        let sources = list_sources(&entries);
        assert_eq!(sources, vec!["a", "b"]);
    }

    #[test]
    fn test_list_destinations() {
        let entries = vec![
            make_entry("a", MapTarget::Move("x".to_string()), vec![]),
            make_entry("b", MapTarget::Mirror, vec![]),
            make_entry("c", MapTarget::Remove, vec![]),
            make_entry("d", MapTarget::Root, vec![]),
        ];
        let dsts = list_destinations(&entries);
        assert_eq!(dsts, vec!["x", "b", "."]);
    }

    // -- add_mapping --

    #[test]
    fn test_add_mapping_new_mirror() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_id_{}", std::process::id()));
        fs::write(&tmp, "").unwrap();
        add_mapping(&tmp, "srcfile", false).unwrap();
        let content = fs::read_to_string(&tmp).unwrap();
        fs::remove_file(&tmp).ok();
        assert!(content.contains("[forward.from \"srcfile\"]"));
        assert!(content.contains("to = (Mirror)"));
    }

    #[test]
    fn test_add_mapping_new_src_dst() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_sd_{}", std::process::id()));
        fs::write(&tmp, "").unwrap();
        add_mapping(&tmp, "src:dest", false).unwrap();
        let content = fs::read_to_string(&tmp).unwrap();
        fs::remove_file(&tmp).ok();
        assert!(content.contains("[forward.from \"src\"]"));
        assert!(content.contains("to = dest"));
    }

    #[test]
    fn test_add_mapping_remove() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_ex_{}", std::process::id()));
        fs::write(&tmp, "").unwrap();
        add_mapping(&tmp, "src:(Remove)", false).unwrap();
        let content = fs::read_to_string(&tmp).unwrap();

        print!("{}", content);
        fs::remove_file(&tmp).ok();
        assert!(content.contains("to = (Remove)"));
    }

    #[test]
    fn test_add_mapping_duplicate_error() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_dup_{}", std::process::id()));
        fs::write(&tmp, "[forward.from \"src\"]\n    to = (Mirror)\n").unwrap();
        let result = add_mapping(&tmp, "src", false);
        fs::remove_file(&tmp).ok();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_add_mapping_duplicate_with_force() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_force_{}", std::process::id()));
        fs::write(&tmp, "[forward.from \"src\"]\n    to = (Mirror)\n").unwrap();
        add_mapping(&tmp, "src", true).unwrap(); // force = true
        fs::remove_file(&tmp).ok();
        // Should not error
    }

    #[test]
    fn test_add_mapping_update_dest_allowed() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_add_upd_{}", std::process::id()));
        fs::write(&tmp, "[forward.from \"src\"]\n    to = old\n").unwrap();
        add_mapping(&tmp, "src:new", false).unwrap(); // has colon, allowed
        let content = fs::read_to_string(&tmp).unwrap();
        fs::remove_file(&tmp).ok();
        assert!(content.contains("to = new"));
    }

    // -- remove_mapping --

    #[test]
    fn test_remove_mapping() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_rm_{}", std::process::id()));
        fs::write(&tmp, "[forward.from \"src\"]\n    to = dest\n\n[forward.from \"other\"]\n    to = other_dest\n")
            .unwrap();
        remove_mapping(&tmp, "src").unwrap();
        let content = fs::read_to_string(&tmp).unwrap();
        fs::remove_file(&tmp).ok();
        assert!(!content.contains("[forward.from \"src\"]"));
        assert!(content.contains("[forward.from \"other\"]"));
    }

    #[test]
    fn test_remove_mapping_not_found() {
        let tmp = std::env::temp_dir().join(format!("pluck_test_rm_nf_{}", std::process::id()));
        fs::write(&tmp, "[forward.from \"src\"]\n    to = dest\n").unwrap();
        let result = remove_mapping(&tmp, "nonexistent");
        fs::remove_file(&tmp).ok();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
