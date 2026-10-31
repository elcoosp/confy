//! Extract and compare project metadata across ecosystem config files.
//!
//! [`ProjectMetadata`] models the common subset of project metadata
//! (name, version, description, authors, license, keywords, dependencies and
//! scripts) declared in four kinds of config files:
//!
//! - `package.json` (Node.js / npm)
//! - `Cargo.toml` (Rust)
//! - `deno.json` (Deno)
//! - `pyproject.toml` (Python)
//!
//! Each ecosystem spells metadata differently; the parser normalises the
//! quirks away so the results can be compared with [`ProjectMetadata::diff`]
//! or persisted as JSON/TOML.
//!
//! # Examples
//!
//! Parsing a config file:
//!
//! ```
//! use confy_project_meta::{ConfigFile, ProjectMetadata};
//!
//! let config = ConfigFile::from_path("tests/package.json")
//!     .expect("package.json is a known config kind");
//! let metadata = ProjectMetadata::from_config(&config).expect("parses");
//! assert_eq!(metadata.name.as_deref(), Some("@elcoosp-configs/biome"));
//! ```
//!
//! Comparing two config files:
//!
//! ```
//! use confy_project_meta::{ConfigFile, ProjectMetadata};
//!
//! let pkg = ProjectMetadata::from_config(&ConfigFile::from_path("tests/package.json").unwrap())
//!     .unwrap();
//! let py = ProjectMetadata::from_config(&ConfigFile::from_path("tests/pyproject.toml").unwrap())
//!     .unwrap();
//! assert!(!pkg.diff(&py).is_empty(), "different projects differ");
//! ```
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A file path is interpreted as one of the four known config file kinds.
///
/// Detect a [`ConfigFile`] from an arbitrary path with
/// [`ConfigFile::from_path`], or list every config file present in a
/// directory with [`ConfigFile::detect_all`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum ConfigFile {
    /// A Node.js `package.json` file.
    PackageJson(PathBuf),
    /// A Rust `Cargo.toml` file.
    CargoToml(PathBuf),
    /// A Deno `deno.json` file.
    DenoJson(PathBuf),
    /// A Python `pyproject.toml` file.
    PyprojectToml(PathBuf),
}

/// The ecosystem a [`ConfigFile`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigKind {
    /// Node.js / npm.
    PackageJson,
    /// Rust / Cargo.
    CargoToml,
    /// Deno.
    DenoJson,
    /// Python / PEP 621.
    PyprojectToml,
}

impl ConfigKind {
    /// The canonical file name of this config kind.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::PackageJson => "package.json",
            Self::CargoToml => "Cargo.toml",
            Self::DenoJson => "deno.json",
            Self::PyprojectToml => "pyproject.toml",
        }
    }
}

impl Display for ConfigKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

impl ConfigFile {
    /// Path of the backing file.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::PackageJson(path)
            | Self::CargoToml(path)
            | Self::DenoJson(path)
            | Self::PyprojectToml(path) => path,
        }
    }

    /// Ecosystem of this config file.
    #[must_use]
    pub const fn kind(&self) -> ConfigKind {
        match self {
            Self::PackageJson(_) => ConfigKind::PackageJson,
            Self::CargoToml(_) => ConfigKind::CargoToml,
            Self::DenoJson(_) => ConfigKind::DenoJson,
            Self::PyprojectToml(_) => ConfigKind::PyprojectToml,
        }
    }

    /// Interpret `path` as a config file based on its final component.
    ///
    /// Returns `None` when the file name does not match one of the four
    /// canonical config file names.
    #[must_use]
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let kind = path.file_name()?.to_str()?;
        match kind {
            "package.json" => Some(Self::PackageJson(path.to_path_buf())),
            "Cargo.toml" => Some(Self::CargoToml(path.to_path_buf())),
            "deno.json" => Some(Self::DenoJson(path.to_path_buf())),
            "pyproject.toml" => Some(Self::PyprojectToml(path.to_path_buf())),
            _ => None,
        }
    }

    /// Every config file present in `dir`, in a fixed ecosystem order:
    /// `package.json`, `Cargo.toml`, `deno.json`, `pyproject.toml`.
    ///
    /// Missing files are skipped silently; a directory containing none
    /// yields an empty list.
    #[must_use]
    pub fn detect_all(dir: impl AsRef<Path>) -> Vec<Self> {
        let dir = dir.as_ref();
        [
            ConfigKind::PackageJson,
            ConfigKind::CargoToml,
            ConfigKind::DenoJson,
            ConfigKind::PyprojectToml,
        ]
        .into_iter()
        .map(|kind| dir.join(kind.file_name()))
        .filter_map(Self::from_path)
        .filter(|config| config.path().is_file())
        .collect()
    }
}

impl Display for ConfigFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path().display())
    }
}

/// Where a dependency is resolved from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencySource {
    /// A version requirement on the ecosystem default registry
    /// (npm, crates.io, JSR, `PyPI`, ...).
    Registry,
    /// A relative or absolute file system path (npm `file:`, Cargo `path`).
    Path,
    /// A git repository (npm `github:`/`git+`, Cargo `git = ...`).
    Git,
    /// A plain URL (npm `https://...`, Deno `https://...`).
    Url,
    /// A workspace member reference (Cargo `workspace = true`).
    Workspace,
}

/// A single dependency entry, normalised across ecosystems.
///
/// The dependency *name* is the map key of
/// [`ProjectMetadata::dependencies`]; [`Dependency`] carries the
/// requirement string and where it resolves from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    /// Version requirement or location specifier, e.g. `^1.2.3`,
    /// `~=1.2.0`, `2.0`, `../sibling` or `user/repo`.
    pub spec: String,
    /// How the spec should be resolved.
    pub source: DependencySource,
}

/// Metadata fields diffable between two [`ProjectMetadata`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataField {
    /// Project name.
    Name,
    /// Project version.
    Version,
    /// Project description.
    Description,
    /// Author list.
    Authors,
    /// License identifier.
    License,
    /// Keyword list.
    Keywords,
    /// Dependency table.
    Dependencies,
    /// Script table.
    Scripts,
}

impl Display for MetadataField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "name",
            Self::Version => "version",
            Self::Description => "description",
            Self::Authors => "authors",
            Self::License => "license",
            Self::Keywords => "keywords",
            Self::Dependencies => "dependencies",
            Self::Scripts => "scripts",
        })
    }
}

/// A single field that differs between two metadata values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMismatch {
    /// Which field differs.
    pub field: MetadataField,
    /// Display rendering of the left-hand value (`<absent>` when unset).
    pub left: String,
    /// Display rendering of the right-hand value (`<absent>` when unset).
    pub right: String,
}

impl Display for FieldMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} vs {}", self.field, self.left, self.right)
    }
}

/// Everything [`ProjectMetadata::from_config`] can go wrong.
///
/// The variants carry display-ready message strings (not source errors) so
/// that the error is serialisable and snapshot friendly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum MetadataError {
    /// The config file does not exist.
    #[error("file not found: {path}")]
    FileNotFound {
        /// Path that was attempted.
        path: String,
    },
    /// The config file exists but could not be read.
    #[error("failed to read {path}: {message}")]
    ReadError {
        /// Path that was read.
        path: String,
        /// Underlying I/O error message.
        message: String,
    },
    /// A JSON based config file (npm, Deno) is not valid JSON.
    #[error("failed to parse {path} as JSON: {message}")]
    JsonParseError {
        /// Path that was parsed.
        path: String,
        /// Parser error message.
        message: String,
    },
    /// A TOML based config file (Cargo, Python) is not valid TOML.
    #[error("failed to parse {path} as TOML: {message}")]
    TomlParseError {
        /// Path that was parsed.
        path: String,
        /// Parser error message.
        message: String,
    },
    /// A path was expected to be one of the four known config kinds.
    #[error("unknown config file kind: {path}")]
    UnknownKind {
        /// Path that could not be classified.
        path: String,
    },
    /// None of the provided config files existed.
    #[error("no configuration files found")]
    NoFilesFound,
    /// Two config files disagree on at least one metadata field.
    #[error("metadata mismatch in {path}: {}", .mismatches.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Mismatch {
        /// Path of the config file that disagrees with the first one.
        path: String,
        /// Per-field differences.
        mismatches: Vec<FieldMismatch>,
    },
}

/// Normalised project metadata extracted from one config file.
///
/// All fields are optional: ecosystems simply do not declare all of them
/// (`deno.json` has no authors, `Cargo.toml` has no scripts, ...).
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct ProjectMetadata {
    /// Project name, e.g. `@elcoosp-configs/biome`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Project version, e.g. `0.0.2`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Short project description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Authors as display strings, e.g. `elcoosp <elcoosp@gmail.com>`.
    ///
    /// Structured author tables (PEP 621) are rendered with the
    /// `Name <email>` convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    /// License identifier or license text, e.g. `MIT`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Project keywords.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    /// Dependencies by name, normalised across ecosystems.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<BTreeMap<String, Dependency>>,
    /// Task runner scripts by name.
    ///
    /// For Deno this is the `tasks` table, for npm the `scripts` table;
    /// Cargo and PEP 621 have no equivalent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<BTreeMap<String, String>>,
}

impl ProjectMetadata {
    /// Parse one config file into metadata.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::FileNotFound`] / [`MetadataError::ReadError`]
    /// when the file cannot be accessed, and
    /// [`MetadataError::JsonParseError`] / [`MetadataError::TomlParseError`]
    /// when its contents are invalid.
    pub fn from_config(config_file: &ConfigFile) -> Result<Self, MetadataError> {
        let path = config_file.path();
        let contents = std::fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                MetadataError::FileNotFound {
                    path: path.display().to_string(),
                }
            } else {
                MetadataError::ReadError {
                    path: path.display().to_string(),
                    message: error.to_string(),
                }
            }
        })?;

        match config_file {
            ConfigFile::PackageJson(_) => Self::from_package_json(path, &contents),
            ConfigFile::CargoToml(_) => Self::from_cargo_toml(path, &contents),
            ConfigFile::DenoJson(_) => Self::from_deno_json(path, &contents),
            ConfigFile::PyprojectToml(_) => Self::from_pyproject_toml(path, &contents),
        }
    }

    fn from_package_json(path: &Path, contents: &str) -> Result<Self, MetadataError> {
        let json: serde_json::Value =
            serde_json::from_str(contents).map_err(|error| MetadataError::JsonParseError {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;

        Ok(Self {
            name: opt_string(&json, &["name"]),
            version: opt_string(&json, &["version"]),
            description: opt_string(&json, &["description"]),
            authors: json_authors(&json),
            license: opt_string(&json, &["license"]),
            keywords: opt_string_list(&json, &["keywords"]),
            dependencies: json
                .get("dependencies")
                .and_then(|value| json_dependencies(value, classify_npm_spec)),
            scripts: json.get("scripts").and_then(string_map),
        })
    }

    fn from_deno_json(path: &Path, contents: &str) -> Result<Self, MetadataError> {
        let json: serde_json::Value =
            serde_json::from_str(contents).map_err(|error| MetadataError::JsonParseError {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;

        Ok(Self {
            name: opt_string(&json, &["name"]),
            version: opt_string(&json, &["version"]),
            description: opt_string(&json, &["description"]),
            // deno.json has no authors field.
            authors: None,
            license: opt_string(&json, &["license"]),
            keywords: opt_string_list(&json, &["keywords"]),
            dependencies: json
                .get("imports")
                .and_then(|value| json_dependencies(value, classify_deno_spec)),
            // Deno calls its scripts table `tasks`.
            scripts: json.get("tasks").and_then(string_map),
        })
    }

    fn from_cargo_toml(path: &Path, contents: &str) -> Result<Self, MetadataError> {
        let toml: toml::Value =
            toml::from_str(contents).map_err(|error| MetadataError::TomlParseError {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        let package = toml.get("package");

        Ok(Self {
            name: toml_opt_string(package, &["name"]),
            version: toml_opt_string(package, &["version"]),
            description: toml_opt_string(package, &["description"]),
            authors: toml_authors(package),
            license: toml_opt_string(package, &["license"]),
            keywords: toml_string_list(package, &["keywords"]),
            dependencies: toml.get("dependencies").and_then(toml_dependencies),
            // Cargo.toml has no scripts field.
            scripts: None,
        })
    }

    fn from_pyproject_toml(path: &Path, contents: &str) -> Result<Self, MetadataError> {
        let toml: toml::Value =
            toml::from_str(contents).map_err(|error| MetadataError::TomlParseError {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        let project = toml.get("project");

        Ok(Self {
            name: toml_opt_string(project, &["name"]),
            version: toml_opt_string(project, &["version"]),
            description: toml_opt_string(project, &["description"]),
            authors: toml_authors(project),
            license: toml_license(project),
            keywords: toml_string_list(project, &["keywords"]),
            dependencies: project
                .and_then(|p| p.get("dependencies"))
                .and_then(toml::Value::as_array)
                .map(|deps| {
                    deps.iter()
                        .filter_map(toml::Value::as_str)
                        .filter_map(split_pep508)
                        .collect::<BTreeMap<String, Dependency>>()
                })
                .filter(|deps: &BTreeMap<String, Dependency>| !deps.is_empty()),
            // pyproject.toml has no scripts field.
            scripts: None,
        })
    }

    /// Parse every provided config file, skipping the ones that do not
    /// exist.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::NoFilesFound`] when no file existed at all,
    /// and propagates read/parse errors of existing files.
    pub fn load_from_config_files(config_files: &[ConfigFile]) -> Result<Vec<Self>, MetadataError> {
        let mut metadata_list = Vec::new();
        for config_file in config_files {
            match Self::from_config(config_file) {
                Ok(metadata) => metadata_list.push(metadata),
                // Missing files are skipped; every other error propagates.
                Err(MetadataError::FileNotFound { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        if metadata_list.is_empty() {
            return Err(MetadataError::NoFilesFound);
        }
        Ok(metadata_list)
    }

    /// Parse every config file found in `dir` (see
    /// [`ConfigFile::detect_all`]).
    ///
    /// # Errors
    ///
    /// Same as [`ProjectMetadata::load_from_config_files`].
    pub fn from_detected_config_files(dir: &Path) -> Result<Vec<Self>, MetadataError> {
        Self::load_from_config_files(&ConfigFile::detect_all(dir))
    }

    /// Compare against another metadata value and list every differing
    /// field.
    ///
    /// An empty result means the two values agree on all fields.
    #[must_use]
    pub fn diff(&self, other: &Self) -> Vec<FieldMismatch> {
        let mut mismatches = Vec::new();
        let mut push = |field: MetadataField, left: String, right: String, differs: bool| {
            if differs {
                mismatches.push(FieldMismatch { field, left, right });
            }
        };

        push(
            MetadataField::Name,
            render_opt(self.name.as_ref()),
            render_opt(other.name.as_ref()),
            self.name != other.name,
        );
        push(
            MetadataField::Version,
            render_opt(self.version.as_ref()),
            render_opt(other.version.as_ref()),
            self.version != other.version,
        );
        push(
            MetadataField::Description,
            render_opt(self.description.as_ref()),
            render_opt(other.description.as_ref()),
            self.description != other.description,
        );
        push(
            MetadataField::Authors,
            render_opt_list(self.authors.as_deref()),
            render_opt_list(other.authors.as_deref()),
            self.authors != other.authors,
        );
        push(
            MetadataField::License,
            render_opt(self.license.as_ref()),
            render_opt(other.license.as_ref()),
            self.license != other.license,
        );
        push(
            MetadataField::Keywords,
            render_opt_list(self.keywords.as_deref()),
            render_opt_list(other.keywords.as_deref()),
            self.keywords != other.keywords,
        );
        push(
            MetadataField::Dependencies,
            render_deps(self.dependencies.as_ref()),
            render_deps(other.dependencies.as_ref()),
            self.dependencies != other.dependencies,
        );
        push(
            MetadataField::Scripts,
            render_scripts(self.scripts.as_ref()),
            render_scripts(other.scripts.as_ref()),
            self.scripts != other.scripts,
        );
        mismatches
    }

    /// Load every config file detected in `dir` and verify they all agree.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::NoFilesFound`] when `dir` contains no known
    /// config file, and [`MetadataError::Mismatch`] describing the first
    /// config file that disagrees with the first-detected one.
    pub fn check_config_files_equality(dir: &Path) -> Result<(), MetadataError> {
        let config_files = ConfigFile::detect_all(dir);
        if config_files.is_empty() {
            return Err(MetadataError::NoFilesFound);
        }

        let metadata_list = Self::load_from_config_files(&config_files)?;
        let first = &metadata_list[0];
        for (config_file, metadata) in config_files.iter().zip(&metadata_list).skip(1) {
            let mismatches = first.diff(metadata);
            if !mismatches.is_empty() {
                return Err(MetadataError::Mismatch {
                    path: config_file.to_string(),
                    mismatches,
                });
            }
        }
        Ok(())
    }
}

fn opt_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

fn opt_string_list(value: &serde_json::Value, keys: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    current.as_array().map(|items| {
        items
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// npm `author` (string) or `authors` (array of strings).
fn json_authors(json: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(author) = json.get("author").and_then(serde_json::Value::as_str) {
        return Some(vec![author.to_string()]);
    }
    json.get("authors")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .filter(|authors: &Vec<String>| !authors.is_empty())
}

/// Normalise one npm-style dependency value into name/spec/source.
fn classify_npm_spec(spec: &str) -> Dependency {
    let source = if spec.starts_with("file:") {
        DependencySource::Path
    } else if spec.starts_with("github:") || spec.starts_with("git+") {
        DependencySource::Git
    } else if spec.starts_with("workspace:") {
        DependencySource::Workspace
    } else if spec.starts_with("http://") || spec.starts_with("https://") {
        DependencySource::Url
    } else {
        DependencySource::Registry
    };
    Dependency {
        spec: spec.to_string(),
        source,
    }
}

/// Normalise one Deno import specifier, stripping the registry scheme.
fn classify_deno_spec(spec: &str) -> Dependency {
    let stripped_url = spec
        .strip_prefix("https://")
        .or_else(|| spec.strip_prefix("http://"));
    if let Some(rest) = stripped_url {
        return Dependency {
            spec: rest.to_string(),
            source: DependencySource::Url,
        };
    }
    let stripped_registry = spec
        .strip_prefix("jsr:")
        .or_else(|| spec.strip_prefix("npm:"));
    Dependency {
        spec: stripped_registry.unwrap_or(spec).to_string(),
        source: DependencySource::Registry,
    }
}

fn json_dependencies(
    value: &serde_json::Value,
    classify: fn(&str) -> Dependency,
) -> Option<BTreeMap<String, Dependency>> {
    let table = value.as_object()?;
    let deps: BTreeMap<String, Dependency> = table
        .iter()
        .map(|(name, spec)| {
            let dependency = spec.as_str().map_or_else(
                // Non-string specs (rare npm extensions) are kept as compact JSON.
                || Dependency {
                    spec: spec.to_string(),
                    source: DependencySource::Registry,
                },
                classify,
            );
            (name.clone(), dependency)
        })
        .collect();
    if deps.is_empty() { None } else { Some(deps) }
}

/// npm `scripts` / Deno `tasks`: object of string to string.
fn string_map(value: &serde_json::Value) -> Option<BTreeMap<String, String>> {
    let table = value.as_object()?;
    let scripts: BTreeMap<String, String> = table
        .iter()
        .filter_map(|(name, task)| task.as_str().map(|task| (name.clone(), task.to_string())))
        .collect();
    if scripts.is_empty() {
        None
    } else {
        Some(scripts)
    }
}

fn toml_opt_string(value: Option<&toml::Value>, keys: &[&str]) -> Option<String> {
    let mut current = value?;
    for key in keys {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

fn toml_string_list(value: Option<&toml::Value>, keys: &[&str]) -> Option<Vec<String>> {
    let mut current = value?;
    for key in keys {
        current = current.get(*key)?;
    }
    match current {
        toml::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect(),
        ),
        // PEP 621 allows a comma separated keyword string.
        toml::Value::String(joined) => Some(
            joined
                .split(',')
                .map(str::trim)
                .filter(|keyword| !keyword.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    }
}

/// Cargo `authors` (strings) or PEP 621 `authors` (tables or strings).
fn toml_authors(value: Option<&toml::Value>) -> Option<Vec<String>> {
    let items = value?.get("authors")?.as_array()?;
    let authors: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            toml::Value::String(author) => Some(author.clone()),
            toml::Value::Table(table) => {
                let name = table.get("name").and_then(toml::Value::as_str);
                let email = table.get("email").and_then(toml::Value::as_str);
                match (name, email) {
                    (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
                    (Some(name), None) => Some(name.to_string()),
                    (None, Some(email)) => Some(format!("<{email}>")),
                    (None, None) => None,
                }
            }
            _ => None,
        })
        .collect();
    if authors.is_empty() {
        None
    } else {
        Some(authors)
    }
}

/// PEP 621 license: SPDX string, `{ text = ... }` or `{ file = ... }`.
fn toml_license(value: Option<&toml::Value>) -> Option<String> {
    match value?.get("license")? {
        toml::Value::String(license) => Some(license.clone()),
        toml::Value::Table(table) => table
            .get("text")
            .or_else(|| table.get("file"))
            .and_then(toml::Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn toml_dependencies(value: &toml::Value) -> Option<BTreeMap<String, Dependency>> {
    let table = value.as_table()?;
    let deps: BTreeMap<String, Dependency> = table
        .iter()
        .map(|(name, spec)| (name.clone(), classify_toml_dep(spec)))
        .collect();
    if deps.is_empty() { None } else { Some(deps) }
}

/// Normalise one Cargo dependency value (string or inline table).
fn classify_toml_dep(spec: &toml::Value) -> Dependency {
    if let Some(version) = spec.as_str() {
        return Dependency {
            spec: version.to_string(),
            source: DependencySource::Registry,
        };
    }
    let Some(table) = spec.as_table() else {
        return Dependency {
            spec: spec.to_string(),
            source: DependencySource::Registry,
        };
    };
    if table
        .get("workspace")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        return Dependency {
            spec: String::new(),
            source: DependencySource::Workspace,
        };
    }
    let pick = |key: &str| table.get(key).and_then(toml::Value::as_str);
    if let Some(path) = pick("path") {
        return Dependency {
            spec: path.to_string(),
            source: DependencySource::Path,
        };
    }
    if let Some(git) = pick("git") {
        return Dependency {
            spec: git.to_string(),
            source: DependencySource::Git,
        };
    }
    Dependency {
        spec: pick("version").unwrap_or_default().to_string(),
        source: DependencySource::Registry,
    }
}

/// Split a PEP 508 requirement into `(name, requirement)`.
///
/// Extras are skipped: `stringcase[extra] ~=1.2.0` yields
/// `("stringcase", "~=1.2.0")`.
fn split_pep508(dep: &str) -> Option<(String, Dependency)> {
    let name: String = dep
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();
    if name.is_empty() {
        return None;
    }
    let rest = dep[name.len()..].trim_start();
    let rest = rest.strip_prefix('[').map_or(rest, |extras| {
        extras
            .find(']')
            .map_or(rest, |end| extras[end + 1..].trim_start())
    });
    let spec = rest.trim();
    Some((
        name,
        Dependency {
            spec: spec.to_string(),
            source: DependencySource::Registry,
        },
    ))
}

fn render_opt(value: Option<&String>) -> String {
    value.map_or_else(|| "<absent>".to_string(), ToString::to_string)
}

fn render_opt_list(items: Option<&[String]>) -> String {
    items.map_or_else(|| "<absent>".to_string(), |items| items.join(", "))
}

fn render_deps(deps: Option<&BTreeMap<String, Dependency>>) -> String {
    deps.map_or_else(
        || "<absent>".to_string(),
        |deps| {
            deps.iter()
                .map(|(name, dep)| format!("{name} {} ({})", dep.spec, dep.source.kind_label()))
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

fn render_scripts(scripts: Option<&BTreeMap<String, String>>) -> String {
    scripts.map_or_else(
        || "<absent>".to_string(),
        |scripts| {
            scripts
                .iter()
                .map(|(name, script)| format!("{name}: {script}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

impl DependencySource {
    /// Short human-readable label used in diff renderings.
    #[must_use]
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::Path => "path",
            Self::Git => "git",
            Self::Url => "url",
            Self::Workspace => "workspace",
        }
    }
}

impl Display for DependencySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_label())
    }
}

#[cfg(test)]
mod tests;
