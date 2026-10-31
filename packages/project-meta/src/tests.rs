use super::*;
use insta::assert_yaml_snapshot;
use std::fs;
use tempfile::tempdir;

const PACKAGE_JSON: &str = include_str!("../tests/package.json");
const CARGO_TOML: &str = include_str!("../tests/Cargo.toml");
const DENO_JSON: &str = include_str!("../tests/deno.json");
const PYPROJECT_TOML: &str = include_str!("../tests/pyproject.toml");

fn write_config(dir: &Path, file_name: &str, contents: &str) -> ConfigFile {
    let path = dir.join(file_name);
    fs::write(&path, contents).unwrap();
    ConfigFile::from_path(&path).unwrap()
}

fn parse_str(file_name: &str, contents: &str) -> Result<ProjectMetadata, MetadataError> {
    let dir = tempdir().unwrap();
    ProjectMetadata::from_config(&write_config(dir.path(), file_name, contents))
}

// ---------------------------------------------------------------------------
// ConfigFile / ConfigKind
// ---------------------------------------------------------------------------

#[test]
fn config_file_accessors_all_kinds() {
    for file_name in ["package.json", "Cargo.toml", "deno.json", "pyproject.toml"] {
        let config = ConfigFile::from_path(file_name).unwrap_or_else(|| {
            panic!("{file_name} should be recognised");
        });
        assert_eq!(config.path(), Path::new(file_name));
        assert_eq!(config.kind().file_name(), file_name);
        assert_eq!(config.to_string(), file_name);
        assert_eq!(config, config.clone());
    }
}

#[test]
fn config_file_from_path_rejects_unknown() {
    assert_eq!(ConfigFile::from_path("README.md"), None);
    assert_eq!(ConfigFile::from_path(""), None);
    assert_eq!(
        ConfigFile::from_path("nested/dir/package.json")
            .unwrap()
            .path(),
        Path::new("nested/dir/package.json")
    );
}

#[test]
fn config_kind_file_names_and_display() {
    assert_eq!(ConfigKind::PackageJson.file_name(), "package.json");
    assert_eq!(ConfigKind::CargoToml.file_name(), "Cargo.toml");
    assert_eq!(ConfigKind::DenoJson.file_name(), "deno.json");
    assert_eq!(ConfigKind::PyprojectToml.file_name(), "pyproject.toml");
    assert_eq!(ConfigKind::PackageJson.to_string(), "package.json");
}

#[test]
fn config_file_serde_roundtrip() {
    let config = ConfigFile::CargoToml(PathBuf::from("a/Cargo.toml"));
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(json, r#"{"kind":"cargo_toml","path":"a/Cargo.toml"}"#);
    assert_eq!(serde_json::from_str::<ConfigFile>(&json).unwrap(), config);
}

#[test]
fn detect_all_finds_all_kinds_in_order() {
    let dir = tempdir().unwrap();
    for (name, contents) in [
        ("package.json", "{}"),
        ("Cargo.toml", ""),
        ("deno.json", "{}"),
        ("pyproject.toml", ""),
    ] {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    let detected = ConfigFile::detect_all(dir.path());
    assert_eq!(
        detected.iter().map(ConfigFile::kind).collect::<Vec<_>>(),
        vec![
            ConfigKind::PackageJson,
            ConfigKind::CargoToml,
            ConfigKind::DenoJson,
            ConfigKind::PyprojectToml,
        ]
    );
}

#[test]
fn detect_all_empty_dir_yields_empty() {
    let dir = tempdir().unwrap();
    assert_eq!(ConfigFile::detect_all(dir.path()), Vec::new());
}

// ---------------------------------------------------------------------------
// Parsing: package.json
// ---------------------------------------------------------------------------

#[test]
fn parses_package_json_fixture() {
    let dir = tempdir().unwrap();
    let metadata =
        ProjectMetadata::from_config(&write_config(dir.path(), "package.json", PACKAGE_JSON))
            .unwrap();
    assert_eq!(metadata.name.as_deref(), Some("@elcoosp-configs/biome"));
    assert_eq!(metadata.version.as_deref(), Some("0.0.2"));
    assert_eq!(
        metadata.description.as_deref(),
        Some("Presets configs for biome")
    );
    assert_eq!(
        metadata.authors,
        Some(vec!["elcoosp <elcoosp@gmail.com>".to_string()])
    );
    assert_eq!(metadata.license, None);
    assert_eq!(metadata.keywords, None);
    assert_eq!(metadata.dependencies, None);
    let scripts = metadata.scripts.as_ref().unwrap();
    assert_eq!(
        scripts.get("build").map(String::as_str),
        Some("rslib build")
    );
    assert_yaml_snapshot!(metadata);
}

#[test]
fn parses_npm_dependency_specs() {
    let metadata = parse_str(
        "package.json",
        r#"{
            "name": "deps",
            "dependencies": {
                "plain": "^1.2.3",
                "local": "file:../local",
                "github": "github:user/repo",
                "gittar": "git+ssh://example.com/repo.git",
                "workspace": "workspace:*",
                "https": "https://example.com/tarball.tgz",
                "object": { "npm": "plain" }
            },
            "scripts": { "build": "echo hi", "broken": 42 }
        }"#,
    )
    .unwrap();
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["plain"].source, DependencySource::Registry);
    assert_eq!(deps["local"].source, DependencySource::Path);
    assert_eq!(deps["github"].source, DependencySource::Git);
    assert_eq!(deps["gittar"].source, DependencySource::Git);
    assert_eq!(deps["workspace"].source, DependencySource::Workspace);
    assert_eq!(deps["https"].source, DependencySource::Url);
    assert_eq!(deps["object"].spec, r#"{"npm":"plain"}"#);
    assert_eq!(deps["object"].source, DependencySource::Registry);
    // The non-string script entry is skipped.
    assert_eq!(metadata.scripts.as_ref().unwrap().len(), 1);
}

#[test]
fn npm_authors_array_and_empty_maps() {
    let metadata = parse_str(
        "package.json",
        r#"{ "authors": ["a", "b"], "dependencies": {}, "scripts": {} }"#,
    )
    .unwrap();
    assert_eq!(
        metadata.authors,
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(metadata.dependencies, None);
    assert_eq!(metadata.scripts, None);
}

#[test]
fn npm_missing_author_and_malformed_authors() {
    let metadata = parse_str("package.json", r#"{ "authors": [1, 2] }"#).unwrap();
    assert_eq!(metadata.authors, None);
}

// ---------------------------------------------------------------------------
// Parsing: deno.json
// ---------------------------------------------------------------------------

#[test]
fn parses_deno_json_fixture() {
    let dir = tempdir().unwrap();
    let metadata =
        ProjectMetadata::from_config(&write_config(dir.path(), "deno.json", DENO_JSON)).unwrap();
    assert_eq!(metadata.name, None);
    assert_eq!(metadata.authors, None);
    assert_eq!(metadata.scripts.as_ref().unwrap().len(), 5);
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["@std/path"].source, DependencySource::Registry);
    assert_eq!(deps["@std/path"].spec, "@std/path@^1.0.8");
    assert_eq!(deps["react"].source, DependencySource::Registry);
    assert_yaml_snapshot!(metadata);
}

#[test]
fn parses_deno_tasks_and_import_schemes() {
    let metadata = parse_str(
        "deno.json",
        r#"{
            "imports": {
                "jsr": "jsr:@std/assert@1.0.8",
                "npm": "npm:react@^18.3.1",
                "url": "https://deno.land/x/mod.ts",
                "bare": "some-bare-spec"
            },
            "tasks": { "check": "deno check", "skipped": [] }
        }"#,
    )
    .unwrap();
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["jsr"].spec, "@std/assert@1.0.8");
    assert_eq!(deps["jsr"].source, DependencySource::Registry);
    assert_eq!(deps["npm"].spec, "react@^18.3.1");
    assert_eq!(deps["url"].source, DependencySource::Url);
    assert_eq!(deps["bare"].source, DependencySource::Registry);
    assert_eq!(deps["bare"].spec, "some-bare-spec");
    // Non-string task values are skipped.
    assert_eq!(metadata.scripts.as_ref().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Parsing: Cargo.toml
// ---------------------------------------------------------------------------

#[test]
fn parses_cargo_toml_fixture() {
    let dir = tempdir().unwrap();
    let metadata =
        ProjectMetadata::from_config(&write_config(dir.path(), "Cargo.toml", CARGO_TOML)).unwrap();
    assert_eq!(metadata.name.as_deref(), Some("biome-configs"));
    assert_eq!(metadata.version.as_deref(), Some("0.0.2"));
    assert_eq!(
        metadata.authors,
        Some(vec!["elcoosp <elcoosp@gmail.com>".to_string()])
    );
    assert_eq!(metadata.license.as_deref(), Some("MIT"));
    assert_eq!(metadata.scripts, None);
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["serde"].source, DependencySource::Registry);
    assert_eq!(deps["serde"].spec, "1.0");
    assert_eq!(deps["local"].source, DependencySource::Path);
    assert_eq!(deps["gitdep"].source, DependencySource::Git);
    assert_eq!(deps["workspace_dep"].source, DependencySource::Workspace);
    assert_yaml_snapshot!(metadata);
}

#[test]
fn parses_cargo_dependency_table_shapes() {
    let metadata = parse_str(
        "Cargo.toml",
        r#"
            [package]
            name = "shapes"

            [dependencies]
            plain = "1.0"
            with_version = { version = "2.1", features = ["x"] }
            only_features = { features = ["x"] }
            workspace_str = { workspace = "yes" }
            integer = 42
        "#,
    )
    .unwrap();
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["plain"].spec, "1.0");
    assert_eq!(deps["with_version"].spec, "2.1");
    assert_eq!(deps["with_version"].source, DependencySource::Registry);
    assert_eq!(deps["only_features"].spec, "");
    assert_eq!(deps["only_features"].source, DependencySource::Registry);
    assert_eq!(deps["workspace_str"].source, DependencySource::Registry);
    assert_eq!(deps["integer"].spec, "42");
    assert_eq!(deps["integer"].source, DependencySource::Registry);
}

#[test]
fn parses_cargo_without_package_table() {
    let metadata = parse_str("Cargo.toml", "[dependencies]\nserde = \"1.0\"\n").unwrap();
    assert_eq!(metadata.name, None);
    assert_eq!(metadata.authors, None);
    assert_eq!(metadata.license, None);
    assert_eq!(metadata.keywords, None);
    assert_eq!(metadata.dependencies.as_ref().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Parsing: pyproject.toml
// ---------------------------------------------------------------------------

#[test]
fn parses_pyproject_fixture() {
    let dir = tempdir().unwrap();
    let metadata =
        ProjectMetadata::from_config(&write_config(dir.path(), "pyproject.toml", PYPROJECT_TOML))
            .unwrap();
    assert_eq!(metadata.name.as_deref(), Some("pyproject-toml-cheat-sheet"));
    assert_eq!(metadata.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        metadata.authors,
        Some(vec!["Your Name <you@yourdomain.com>".to_string()])
    );
    assert_eq!(metadata.license.as_deref(), Some("MIT License"));
    assert_eq!(metadata.keywords, Some(vec!["somepyproj".to_string()]));
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["stringcase"].spec, "~=1.2.0");
    assert_eq!(deps["stringcase"].source, DependencySource::Registry);
    assert_eq!(metadata.scripts, None);
    assert_yaml_snapshot!(metadata);
}

#[test]
fn parses_pyproject_author_shapes_and_license_variants() {
    let metadata = parse_str(
        "pyproject.toml",
        r#"
            [project]
            name = "shapes"
            authors = [
                { name = "full" },
                { email = "bare@example.com" },
                "plain string",
                42,
            ]
            license = { file = "LICENSE.txt" }
            keywords = "kw1, kw2"
            dependencies = [
                "name[extra] >=1.2",
                "bare-dep",
                "~=1.2.0",
                42,
            ]
        "#,
    )
    .unwrap();
    assert_eq!(
        metadata.authors,
        Some(vec![
            "full".to_string(),
            "<bare@example.com>".to_string(),
            "plain string".to_string(),
        ])
    );
    assert_eq!(metadata.license.as_deref(), Some("LICENSE.txt"));
    assert_eq!(
        metadata.keywords,
        Some(vec!["kw1".to_string(), "kw2".to_string()])
    );
    let deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(deps["name"].spec, ">=1.2");
    assert_eq!(deps["bare-dep"].spec, "");
    // The integer requirement is skipped by the PEP 508 parser.
    assert_eq!(deps.len(), 2);
}

#[test]
fn parses_pyproject_spdx_license_and_no_project() {
    let metadata = parse_str("pyproject.toml", "[project]\nlicense = \"MIT\"\n").unwrap();
    assert_eq!(metadata.license.as_deref(), Some("MIT"));
    assert_eq!(metadata.dependencies, None);

    let empty = parse_str("pyproject.toml", "[tool.other]\nkey = 1\n").unwrap();
    assert_eq!(empty.name, None);
    assert_eq!(empty.license, None);
    assert_eq!(empty.keywords, None);
    assert_eq!(empty.authors, None);
    assert_eq!(empty.dependencies, None);
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn missing_file_is_file_not_found() {
    let dir = tempdir().unwrap();
    let error =
        ProjectMetadata::from_config(&ConfigFile::PackageJson(dir.path().join("package.json")))
            .unwrap_err();
    assert_eq!(
        error,
        MetadataError::FileNotFound {
            path: dir.path().join("package.json").display().to_string()
        }
    );
}

#[test]
fn directory_path_is_read_error() {
    let dir = tempdir().unwrap();
    let error =
        ProjectMetadata::from_config(&ConfigFile::CargoToml(dir.path().to_path_buf())).unwrap_err();
    assert!(matches!(error, MetadataError::ReadError { .. }));
}

#[test]
fn invalid_json_and_toml_are_reported() {
    let dir = tempdir().unwrap();
    let json_error =
        ProjectMetadata::from_config(&write_config(dir.path(), "package.json", "{ nope"))
            .unwrap_err();
    assert!(matches!(json_error, MetadataError::JsonParseError { .. }));
    let toml_error =
        ProjectMetadata::from_config(&write_config(dir.path(), "Cargo.toml", "[[[")).unwrap_err();
    assert!(matches!(toml_error, MetadataError::TomlParseError { .. }));
    let pyproject_error =
        ProjectMetadata::from_config(&write_config(dir.path(), "pyproject.toml", "[[["))
            .unwrap_err();
    assert!(matches!(
        pyproject_error,
        MetadataError::TomlParseError { .. }
    ));
}

#[test]
fn error_displays_are_informative() {
    let cases = [
        (
            MetadataError::FileNotFound { path: "a".into() },
            "file not found: a",
        ),
        (
            MetadataError::ReadError {
                path: "a".into(),
                message: "boom".into(),
            },
            "failed to read a: boom",
        ),
        (
            MetadataError::JsonParseError {
                path: "a".into(),
                message: "bad".into(),
            },
            "failed to parse a as JSON: bad",
        ),
        (
            MetadataError::TomlParseError {
                path: "a".into(),
                message: "bad".into(),
            },
            "failed to parse a as TOML: bad",
        ),
        (
            MetadataError::UnknownKind { path: "a".into() },
            "unknown config file kind: a",
        ),
        (MetadataError::NoFilesFound, "no configuration files found"),
        (
            MetadataError::Mismatch {
                path: "b".into(),
                mismatches: vec![FieldMismatch {
                    field: MetadataField::Name,
                    left: "x".into(),
                    right: "y".into(),
                }],
            },
            "metadata mismatch in b: name: x vs y",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn pyproject_unsupported_field_types_are_ignored() {
    let metadata = parse_str(
        "pyproject.toml",
        r#"
            [project]
            name = "edges"
            authors = [{ nickname = "x" }]
            license = 42
            keywords = 42
        "#,
    )
    .unwrap();
    // Author tables without name/email yield None entries.
    assert_eq!(metadata.authors, None);
    assert_eq!(metadata.license, None);
    assert_eq!(metadata.keywords, None);
}

#[test]
fn pyproject_empty_authors_list_is_none() {
    let metadata = parse_str("pyproject.toml", "[project]\nauthors = []\n").unwrap();
    assert_eq!(metadata.authors, None);
}

// ---------------------------------------------------------------------------
// Loading, detection, diffing
// ---------------------------------------------------------------------------

#[test]
fn load_skips_missing_and_errors_when_none_found() {
    let dir = tempdir().unwrap();
    let pkg = write_config(dir.path(), "package.json", PACKAGE_JSON);
    let missing = ConfigFile::CargoToml(dir.path().join("nope.toml"));
    let loaded = ProjectMetadata::load_from_config_files(&[missing, pkg.clone()]).unwrap();
    assert_eq!(loaded.len(), 1);

    let all_missing = [ConfigFile::CargoToml(dir.path().join("nope.toml"))];
    assert_eq!(
        ProjectMetadata::load_from_config_files(&all_missing).unwrap_err(),
        MetadataError::NoFilesFound
    );

    let broken = write_config(dir.path(), "deno.json", "{ nope");
    let broken_result = ProjectMetadata::load_from_config_files(&[broken, pkg]);
    assert!(matches!(
        broken_result.unwrap_err(),
        MetadataError::JsonParseError { .. }
    ));
}

#[test]
fn from_detected_config_files_works_on_tempdir() {
    let dir = tempdir().unwrap();
    assert_eq!(
        ProjectMetadata::from_detected_config_files(dir.path()).unwrap_err(),
        MetadataError::NoFilesFound
    );
    fs::write(dir.path().join("package.json"), PACKAGE_JSON).unwrap();
    assert_eq!(
        ProjectMetadata::from_detected_config_files(dir.path())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn equality_check_passes_and_fails() {
    let dir = tempdir().unwrap();
    // No files at all -> NoFilesFound.
    assert_eq!(
        ProjectMetadata::check_config_files_equality(dir.path()).unwrap_err(),
        MetadataError::NoFilesFound
    );

    // Two files with identical metadata -> Ok.
    let shared = r#"{ "name": "x", "version": "1.0" }"#;
    fs::write(dir.path().join("package.json"), shared).unwrap();
    fs::write(dir.path().join("deno.json"), shared).unwrap();
    ProjectMetadata::check_config_files_equality(dir.path()).unwrap();

    // One differing field -> Mismatch pointing at the second file.
    fs::write(
        dir.path().join("deno.json"),
        r#"{ "name": "x", "version": "2.0" }"#,
    )
    .unwrap();
    let error = ProjectMetadata::check_config_files_equality(dir.path()).unwrap_err();
    match error {
        MetadataError::Mismatch { path, mismatches } => {
            assert!(path.ends_with("deno.json"));
            assert_eq!(mismatches.len(), 1);
            assert_eq!(mismatches[0].field, MetadataField::Version);
        }
        other => panic!("expected mismatch, got {other:?}"),
    }
}

#[test]
fn diff_reports_every_differing_field() {
    let left = parse_str(
        "package.json",
        r#"{ "name": "a", "version": "1", "description": "d", "author": "me",
             "license": "MIT", "keywords": ["k"], "dependencies": { "x": "^1" },
             "scripts": { "b": "echo" } }"#,
    )
    .unwrap();
    let right = parse_str(
        "package.json",
        r#"{ "name": "b", "version": "2", "description": "e", "authors": ["you"],
             "license": "Apache-2.0", "keywords": ["j"], "dependencies": { "y": "^2" },
             "scripts": { "c": "run" } }"#,
    )
    .unwrap();

    let mismatches = left.diff(&right);
    assert_eq!(mismatches.len(), 8);
    assert_eq!(left.diff(&left), vec![]);

    let rendered = mismatches
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(rendered[0].contains("name: a vs b"));
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("authors: me vs you"))
    );
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("dependencies: x ^1 (registry) vs y ^2 (registry)"))
    );
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("scripts: b: echo vs c: run"))
    );
}

#[test]
fn diff_renders_absent_values() {
    let left = ProjectMetadata::default();
    let right = ProjectMetadata {
        name: Some("present".to_string()),
        authors: Some(vec!["a".to_string()]),
        dependencies: Some(BTreeMap::new()),
        scripts: Some(BTreeMap::new()),
        ..ProjectMetadata::default()
    };
    let mismatches = left.diff(&right);
    // Only the four populated fields differ; the rest are absent on both sides.
    assert_eq!(mismatches.len(), 4);
    let rendered = mismatches
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("<absent> vs present"))
    );
    assert!(rendered.iter().any(|line| line.contains("vs a")));
}

#[test]
fn metadata_field_display_covers_all() {
    let fields = [
        (MetadataField::Name, "name"),
        (MetadataField::Version, "version"),
        (MetadataField::Description, "description"),
        (MetadataField::Authors, "authors"),
        (MetadataField::License, "license"),
        (MetadataField::Keywords, "keywords"),
        (MetadataField::Dependencies, "dependencies"),
        (MetadataField::Scripts, "scripts"),
    ];
    for (field, name) in fields {
        assert_eq!(field.to_string(), name);
    }
}

#[test]
fn dependency_source_labels_covers_all() {
    let sources = [
        (DependencySource::Registry, "registry"),
        (DependencySource::Path, "path"),
        (DependencySource::Git, "git"),
        (DependencySource::Url, "url"),
        (DependencySource::Workspace, "workspace"),
    ];
    for (source, label) in sources {
        assert_eq!(source.kind_label(), label);
        assert_eq!(source.to_string(), label);
    }
}

#[test]
fn pep508_edge_cases() {
    // Unclosed extras block keeps the raw remainder.
    let (name, dep) = split_pep508("weird[unclosed").unwrap();
    assert_eq!(name, "weird");
    assert_eq!(dep.spec, "[unclosed");
    // No leading name character at all.
    assert!(split_pep508("~=1.2.0").is_none());
    assert!(split_pep508("").is_none());
}

// ---------------------------------------------------------------------------
// Snapshot combinations
// ---------------------------------------------------------------------------

macro_rules! generate_snapshot_test {
    ($test_name:ident, $config_files:expr) => {
        #[test]
        fn $test_name() {
            let dir = tempdir().unwrap();
            let config_files: Vec<ConfigFile> = $config_files
                .iter()
                .map(|(file_name, contents)| write_config(dir.path(), file_name, contents))
                .collect();
            let result = ProjectMetadata::load_from_config_files(&config_files);
            assert_yaml_snapshot!(result);
        }
    };
}

generate_snapshot_test!(
    snapshot_pkg_and_cargo,
    [("package.json", PACKAGE_JSON), ("Cargo.toml", CARGO_TOML),]
);
generate_snapshot_test!(
    snapshot_deno_and_pyproject,
    [("deno.json", DENO_JSON), ("pyproject.toml", PYPROJECT_TOML),]
);
#[test]
fn snapshot_missing_file() {
    let dir = tempdir().unwrap();
    let result = ProjectMetadata::load_from_config_files(&[ConfigFile::CargoToml(
        dir.path().join("nope.toml"),
    )]);
    assert_yaml_snapshot!(result);
}
