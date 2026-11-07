use super::*;
use confy_project_meta::MetadataField;
use std::fs;
use tempfile::tempdir;

const PACKAGE_JSON: &str = r#"{ "name": "root-pkg", "version": "1.0.0" }"#;
const PACKAGE_JSON_OTHER: &str = r#"{ "name": "other-pkg", "version": "2.0.0" }"#;
const CARGO_TOML: &str = r#"
    [package]
    name = "root-crate"
    version = "1.0.0"
    license = "MIT"
"#;
const DENO_JSON: &str = r#"{ "name": "root-pkg", "version": "1.0.0" }"#;
const DENO_JSON_WITH_TASKS: &str =
    r#"{ "name": "root-pkg", "version": "1.0.0", "tasks": { "check": "deno check" } }"#;
const PYPROJECT_TOML: &str = r#"
    [project]
    name = "root-pkg"
    version = "1.0.0"
"#;
const BROKEN_JSON: &str = "{ nope";

/// Create `dir/sub` containing the given config files.
fn write_subdir(root: &Path, sub: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = root.join(sub);
    fs::create_dir_all(&dir).unwrap();
    for (name, contents) in files {
        fs::write(dir.join(name), contents).unwrap();
    }
    dir
}

/// Render discovered paths relative to `root` for readable assertions.
fn relative(configs: &[ConfigFile], root: &Path) -> Vec<String> {
    configs
        .iter()
        .map(|config| {
            config
                .path()
                .strip_prefix(root)
                .unwrap()
                .display()
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ScanOptions
// ---------------------------------------------------------------------------

#[test]
fn scan_options_defaults() {
    let options = ScanOptions::default();
    assert_eq!(options.max_depth, 64);
    assert!(!options.include_hidden);
    assert_eq!(options.extra_ignored_dirs, Vec::<String>::new());
}

#[test]
fn scan_options_builders_chain() {
    let options = ScanOptions::default()
        .with_max_depth(2)
        .with_include_hidden(true)
        .with_extra_ignored_dirs(vec!["vendor".to_string(), "generated".to_string()]);
    assert_eq!(options.max_depth, 2);
    assert!(options.include_hidden);
    assert_eq!(
        options.extra_ignored_dirs,
        vec!["vendor".to_string(), "generated".to_string()]
    );
}

#[test]
fn default_ignored_dirs_cover_common_build_dirs() {
    for name in [
        ".git",
        "node_modules",
        "target",
        "dist",
        "build",
        "__pycache__",
    ] {
        assert!(
            DEFAULT_IGNORED_DIRS.contains(&name),
            "{name} should be ignored by default"
        );
    }
}

// ---------------------------------------------------------------------------
// discover
// ---------------------------------------------------------------------------

#[test]
fn discover_empty_dir_yields_no_configs() {
    let dir = tempdir().unwrap();
    assert_eq!(
        ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap(),
        Vec::new()
    );
}

#[test]
fn discover_finds_root_configs_in_ecosystem_order() {
    let dir = tempdir().unwrap();
    for (name, contents) in [
        ("package.json", PACKAGE_JSON),
        ("Cargo.toml", CARGO_TOML),
        ("deno.json", DENO_JSON),
        ("pyproject.toml", PYPROJECT_TOML),
    ] {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    let discovered = ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(
        discovered.iter().map(ConfigFile::kind).collect::<Vec<_>>(),
        vec![
            ConfigKind::PackageJson,
            ConfigKind::CargoToml,
            ConfigKind::DenoJson,
            ConfigKind::PyprojectToml,
        ]
    );
}

#[test]
fn discover_walks_nested_dirs_deterministically() {
    let dir = tempdir().unwrap();
    write_subdir(
        dir.path(),
        "apps/web",
        &[("package.json", PACKAGE_JSON_OTHER)],
    );
    write_subdir(dir.path(), "crates/core", &[("Cargo.toml", CARGO_TOML)]);
    write_subdir(dir.path(), "zeta", &[("deno.json", DENO_JSON)]);
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();

    let first = ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap();
    let second = ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(first, second, "discovery is deterministic");
    // Parent configs first, then sub directories in lexicographic order.
    assert_eq!(
        relative(&first, dir.path()),
        vec![
            "package.json",
            "apps/web/package.json",
            "crates/core/Cargo.toml",
            "zeta/deno.json",
        ]
    );
}

#[test]
fn discover_prunes_default_and_extra_ignored_dirs() {
    let dir = tempdir().unwrap();
    write_subdir(
        dir.path(),
        "node_modules/pkg",
        &[("package.json", PACKAGE_JSON)],
    );
    write_subdir(dir.path(), "target/debug", &[("Cargo.toml", CARGO_TOML)]);
    write_subdir(dir.path(), ".git/hooks", &[("deno.json", DENO_JSON)]);
    write_subdir(dir.path(), "vendor/lib", &[("package.json", PACKAGE_JSON)]);
    write_subdir(dir.path(), "kept", &[("package.json", PACKAGE_JSON)]);

    let options = ScanOptions::default().with_extra_ignored_dirs(vec!["vendor".to_string()]);
    let discovered = ProjectTree::discover(dir.path(), &options).unwrap();
    assert_eq!(relative(&discovered, dir.path()), vec!["kept/package.json"]);
}

#[test]
fn discover_skips_hidden_dirs_unless_included() {
    let dir = tempdir().unwrap();
    write_subdir(dir.path(), ".hidden", &[("package.json", PACKAGE_JSON)]);

    let options = ScanOptions::default();
    assert_eq!(
        ProjectTree::discover(dir.path(), &options).unwrap(),
        Vec::new()
    );

    let options = ScanOptions::default().with_include_hidden(true);
    let discovered = ProjectTree::discover(dir.path(), &options).unwrap();
    assert_eq!(discovered.len(), 1);
    assert!(discovered[0].to_string().ends_with(".hidden/package.json"));
}

#[test]
fn discover_respects_max_depth() {
    let dir = tempdir().unwrap();
    write_subdir(dir.path(), "a/b", &[("package.json", PACKAGE_JSON)]);

    let options = ScanOptions::default().with_max_depth(0);
    assert_eq!(
        ProjectTree::discover(dir.path(), &options).unwrap(),
        Vec::new()
    );

    let options = ScanOptions::default().with_max_depth(1);
    assert_eq!(
        ProjectTree::discover(dir.path(), &options).unwrap(),
        Vec::new()
    );

    let options = ScanOptions::default().with_max_depth(2);
    assert_eq!(
        ProjectTree::discover(dir.path(), &options).unwrap().len(),
        1
    );
}

#[test]
fn discover_skips_config_named_directories() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("package.json")).unwrap();
    assert_eq!(
        ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap(),
        Vec::new()
    );
}

#[cfg(unix)]
#[test]
fn discover_skips_symlinked_directories() {
    let dir = tempdir().unwrap();
    let target = write_subdir(dir.path(), "real", &[("package.json", PACKAGE_JSON)]);
    std::os::unix::fs::symlink(&target, dir.path().join("link")).unwrap();
    let discovered = ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap();
    // The real directory is scanned; the symlink is never descended into.
    assert_eq!(relative(&discovered, dir.path()), vec!["real/package.json"]);
    assert!(!discovered[0].to_string().contains("link/package.json"));
}

#[test]
fn discover_rejects_missing_root_and_file_root() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing");
    assert_eq!(
        ProjectTree::discover(&missing, &ScanOptions::default()).unwrap_err(),
        TreeError::RootNotFound {
            path: missing.display().to_string()
        }
    );

    let file = dir.path().join("plain.txt");
    fs::write(&file, "text").unwrap();
    assert_eq!(
        ProjectTree::discover(&file, &ScanOptions::default()).unwrap_err(),
        TreeError::RootNotDirectory {
            path: file.display().to_string()
        }
    );
}

// ---------------------------------------------------------------------------
// scan / ProjectTree
// ---------------------------------------------------------------------------

#[test]
fn scan_empty_dir_yields_empty_tree() {
    let dir = tempdir().unwrap();
    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.groups, Vec::new());
    assert_eq!(tree.issues, Vec::new());
    assert_eq!(tree.root, dir.path());
}

#[test]
fn scan_groups_configs_by_directory() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    fs::write(dir.path().join("deno.json"), DENO_JSON_WITH_TASKS).unwrap();
    write_subdir(dir.path(), "crates/core", &[("Cargo.toml", CARGO_TOML)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.groups.len(), 2);
    assert_eq!(tree.groups[0].dir, dir.path());
    assert_eq!(tree.groups[1].dir, dir.path().join("crates/core"));

    let root_group = &tree.groups[0];
    assert_eq!(
        root_group
            .entries
            .iter()
            .map(|entry| entry.config.kind())
            .collect::<Vec<_>>(),
        vec![ConfigKind::PackageJson, ConfigKind::DenoJson]
    );
    let deno_metadata = root_group.entries[1].metadata.as_ref().unwrap();
    assert_eq!(
        deno_metadata.scripts.as_ref().unwrap().get("check"),
        Some(&"deno check".to_string())
    );
    assert_eq!(tree.issues, Vec::new());
}

#[test]
fn scan_records_parse_issues_and_keeps_going() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    write_subdir(dir.path(), "broken", &[("package.json", BROKEN_JSON)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.groups.len(), 2);
    // The broken directory has its entry, with metadata set to None.
    let broken_group = &tree.groups[1];
    assert_eq!(broken_group.entries.len(), 1);
    assert_eq!(broken_group.entries[0].metadata, None);

    assert_eq!(tree.issues.len(), 1);
    assert_eq!(tree.issues[0].kind, IssueKind::Parse);
    assert!(tree.issues[0].path.ends_with("broken/package.json"));
    assert!(tree.issues[0].message.contains("as JSON"));
}

#[test]
fn scan_roundtrips_through_json() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    write_subdir(dir.path(), "broken", &[("deno.json", BROKEN_JSON)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    let json = serde_json::to_string(&tree).unwrap();
    let restored: ProjectTree = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, tree);
}

#[test]
fn scan_is_deterministic() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    write_subdir(dir.path(), "b", &[("deno.json", DENO_JSON)]);
    write_subdir(dir.path(), "a", &[("Cargo.toml", CARGO_TOML)]);

    let first = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    let second = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(first, second);
}

#[cfg(unix)]
#[test]
fn scan_surfaces_walk_errors_as_issues() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    write_subdir(dir.path(), "locked", &[]);
    fs::set_permissions(dir.path().join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.issues.len(), 1);
    assert_eq!(tree.issues[0].kind, IssueKind::Walk);
    assert!(tree.issues[0].path.ends_with("locked"));
    assert_ne!(tree.issues[0].message, "");

    // Restore so the temp dir can be removed.
    fs::set_permissions(dir.path().join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn discover_descends_into_non_utf8_dir_names() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempdir().unwrap();
    let weird = dir.path().join(OsStr::from_bytes(b"we\xffird"));
    fs::create_dir_all(&weird).unwrap();
    fs::write(weird.join("package.json"), PACKAGE_JSON).unwrap();

    // A non-UTF-8 name is neither hidden nor on any ignore list, so the
    // directory is scanned normally.
    let discovered = ProjectTree::discover(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(discovered.len(), 1);
    assert!(discovered[0].to_string().contains("package.json"));
}

#[test]
fn duplicate_names_skips_unnamed_packages() {
    let dir = tempdir().unwrap();
    // A Cargo.toml without a [package] name (e.g. a workspace root
    // manifest) contributes no name.
    write_subdir(
        dir.path(),
        "workspace-root",
        &[("Cargo.toml", "[workspace]\n")],
    );
    write_subdir(
        dir.path(),
        "workspace-root/member",
        &[("Cargo.toml", "[workspace]\n")],
    );

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.groups.len(), 2);
    assert_eq!(tree.duplicate_names(), Vec::new());
}

// ---------------------------------------------------------------------------
// Issue rendering
// ---------------------------------------------------------------------------

#[test]
fn issue_display_renders_both_kinds() {
    let walk = TreeIssue {
        kind: IssueKind::Walk,
        path: "locked".to_string(),
        message: "permission denied".to_string(),
    };
    assert_eq!(walk.to_string(), "walk error at locked: permission denied");

    let parse = TreeIssue {
        kind: IssueKind::Parse,
        path: "deno.json".to_string(),
        message: "boom".to_string(),
    };
    assert_eq!(parse.to_string(), "failed to parse deno.json: boom");
}

#[test]
fn issue_kind_display() {
    assert_eq!(IssueKind::Walk.to_string(), "walk");
    assert_eq!(IssueKind::Parse.to_string(), "parse");
}

#[test]
fn tree_error_display() {
    assert_eq!(
        TreeError::RootNotFound {
            path: "gone".to_string()
        }
        .to_string(),
        "root path not found: gone"
    );
    assert_eq!(
        TreeError::RootNotDirectory {
            path: "file".to_string()
        }
        .to_string(),
        "root path is not a directory: file"
    );
}

// ---------------------------------------------------------------------------
// check_group_consistency
// ---------------------------------------------------------------------------

#[test]
fn consistency_passes_when_groups_agree() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    fs::write(dir.path().join("deno.json"), DENO_JSON).unwrap();
    write_subdir(
        dir.path(),
        "apps/web",
        &[("package.json", PACKAGE_JSON_OTHER)],
    );

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.check_group_consistency(), Vec::new());
}

#[test]
fn consistency_reports_every_disagreeing_group() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    // Same name but a different version: one mismatch on `version`.
    fs::write(
        dir.path().join("deno.json"),
        r#"{ "name": "root-pkg", "version": "9.9.9" }"#,
    )
    .unwrap();
    write_subdir(dir.path(), "sub", &[("package.json", PACKAGE_JSON)]);
    fs::write(
        dir.path().join("sub/deno.json"),
        r#"{ "name": "root-pkg", "version": "2.0.0" }"#,
    )
    .unwrap();

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    let mismatches = tree.check_group_consistency();
    assert_eq!(mismatches.len(), 2);
    assert!(mismatches[0].group_dir.ends_with("sub") || mismatches[1].group_dir.ends_with("sub"));
    for mismatch in &mismatches {
        assert_eq!(mismatch.mismatches.len(), 1);
        assert_eq!(mismatch.mismatches[0].field, MetadataField::Version);
    }
    let rendered = mismatches[0].to_string();
    assert!(rendered.contains("metadata mismatch in"));
    assert!(rendered.contains("deno.json"));
    assert!(rendered.contains("version: 1.0.0 vs 9.9.9"));
}

#[test]
fn consistency_skips_groups_without_two_parsed_entries() {
    let dir = tempdir().unwrap();
    // Single entry.
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    // All entries failed to parse.
    write_subdir(dir.path(), "broken", &[("deno.json", BROKEN_JSON)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.groups.len(), 2);
    assert_eq!(tree.check_group_consistency(), Vec::new());
}

// ---------------------------------------------------------------------------
// duplicate_names
// ---------------------------------------------------------------------------

#[test]
fn duplicate_names_reports_cross_group_clashes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    write_subdir(dir.path(), "sibling", &[("pyproject.toml", PYPROJECT_TOML)]);
    write_subdir(dir.path(), "unique", &[("Cargo.toml", CARGO_TOML)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    let duplicates = tree.duplicate_names();
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].name, "root-pkg");
    assert_eq!(duplicates[0].paths.len(), 2);
    let rendered = duplicates[0].to_string();
    assert!(rendered.contains("root-pkg"));
    assert!(rendered.contains("duplicate package name"));
}

#[test]
fn duplicate_names_ignores_within_group_and_unnamed() {
    let dir = tempdir().unwrap();
    // Same name in two files of the SAME group: not reported here
    // (intra-group agreement is the job of check_group_consistency).
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    fs::write(dir.path().join("pyproject.toml"), PYPROJECT_TOML).unwrap();

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(tree.duplicate_names(), Vec::new());
}

#[test]
fn duplicate_names_collapses_three_occurrences() {
    let dir = tempdir().unwrap();
    write_subdir(dir.path(), "one", &[("package.json", PACKAGE_JSON)]);
    write_subdir(dir.path(), "two", &[("deno.json", DENO_JSON)]);
    write_subdir(dir.path(), "three", &[("pyproject.toml", PYPROJECT_TOML)]);

    let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
    let duplicates = tree.duplicate_names();
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].paths.len(), 3);
}
