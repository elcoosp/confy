//! Repository-scale scanning for the four known project config files.
//!
//! [`confy_project_meta`] works on single directories:
//! [`ConfigFile::detect_all`](confy_project_meta::ConfigFile::detect_all)
//! finds the config files of one folder and
//! [`check_config_files_equality`](confy_project_meta::ProjectMetadata::check_config_files_equality)
//! verifies the agreement of exactly that folder. Real repositories are
//! trees: an npm workspace with nested packages, a Cargo workspace with
//! crates in `crates/`, a Python package next to a Deno tool. This crate
//! adds that missing layer on top of `confy-project-meta`:
//!
//! - [`ProjectTree::discover`] walks a directory tree and returns every
//!   `package.json`, `Cargo.toml`, `deno.json` and `pyproject.toml` it
//!   finds, skipping build artefacts and vendored code
//!   ([`DEFAULT_IGNORED_DIRS`]).
//! - [`ProjectTree::scan`] additionally parses every discovered file with
//!   [`ProjectMetadata`] and groups the results per directory; broken
//!   files are recorded as [`TreeIssue`]s instead of aborting the walk.
//! - [`ProjectTree::check_group_consistency`] verifies that the config
//!   files of *every* directory agree, the whole-tree equivalent of
//!   `check_config_files_equality`.
//! - [`ProjectTree::duplicate_names`] flags package names that are
//!   declared by more than one directory, a common monorepo mistake.
//!
//! All report types are serialisable, so the tree can be rendered as JSON
//! or TOML by the `confy scan` CLI command.
//!
//! # Examples
//!
//! Scanning a repository tree:
//!
//! ```
//! use confy_project_tree::{ProjectTree, ScanOptions};
//! use tempfile::tempdir;
//! use std::fs;
//!
//! let dir = tempdir().unwrap();
//! fs::write(dir.path().join("package.json"), r#"{ "name": "app" }"#).unwrap();
//! fs::create_dir_all(dir.path().join("crates/lib")).unwrap();
//! fs::write(
//!     dir.path().join("crates/lib/Cargo.toml"),
//!     "[package]\nname = \"lib\"\n",
//! )
//! .unwrap();
//!
//! let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
//! assert_eq!(tree.groups.len(), 2);
//! assert!(tree.issues.is_empty());
//! assert_eq!(tree.duplicate_names(), Vec::new());
//! ```
//!
//! Consistency checking across the whole tree:
//!
//! ```
//! use confy_project_tree::{ProjectTree, ScanOptions};
//! use tempfile::tempdir;
//! use std::fs;
//!
//! let dir = tempdir().unwrap();
//! let shared = r#"{ "name": "app", "version": "1.0.0" }"#;
//! fs::write(dir.path().join("package.json"), shared).unwrap();
//! fs::write(dir.path().join("deno.json"), shared).unwrap();
//!
//! let tree = ProjectTree::scan(dir.path(), &ScanOptions::default()).unwrap();
//! assert_eq!(tree.check_group_consistency(), Vec::new());
//! ```
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use confy_project_meta::{ConfigFile, ConfigKind, FieldMismatch, ProjectMetadata};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Directory names that [`ProjectTree::discover`] never descends into.
///
/// These cover version control internals, dependency folders, build
/// outputs and tool caches of the four supported ecosystems. Names are
/// matched against the whole directory name; additional names can be
/// ignored per scan with
/// [`ScanOptions::with_extra_ignored_dirs`].
pub const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "venv",
    ".venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".next",
    ".turbo",
    "coverage",
    ".nyc_output",
];

/// Options controlling how [`ProjectTree::discover`] and
/// [`ProjectTree::scan`] walk the directory tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// Maximum directory depth relative to the root, `0` scans only the
    /// root directory itself.
    pub max_depth: usize,
    /// Whether to descend into hidden directories (dot-prefixed names).
    /// Entries of [`DEFAULT_IGNORED_DIRS`] stay ignored either way.
    pub include_hidden: bool,
    /// Additional directory names to prune, on top of
    /// [`DEFAULT_IGNORED_DIRS`].
    pub extra_ignored_dirs: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 64,
            include_hidden: false,
            extra_ignored_dirs: Vec::new(),
        }
    }
}

impl ScanOptions {
    /// Set the maximum scan depth.
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Whether to descend into hidden directories.
    #[must_use]
    pub const fn with_include_hidden(mut self, include_hidden: bool) -> Self {
        self.include_hidden = include_hidden;
        self
    }

    /// Append additional directory names to prune.
    #[must_use]
    pub fn with_extra_ignored_dirs(mut self, extra_ignored_dirs: Vec<String>) -> Self {
        self.extra_ignored_dirs.extend(extra_ignored_dirs);
        self
    }

    /// Whether a directory with this name should be pruned.
    fn is_ignored_dir(&self, name: &OsStr) -> bool {
        let Some(name) = name.to_str() else {
            return false;
        };
        if DEFAULT_IGNORED_DIRS.contains(&name) {
            return true;
        }
        if self
            .extra_ignored_dirs
            .iter()
            .any(|ignored| ignored == name)
        {
            return true;
        }
        !self.include_hidden && name.starts_with('.')
    }
}

/// Why a [`ProjectTree`] could not be produced at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum TreeError {
    /// The scan root does not exist.
    #[error("root path not found: {path}")]
    RootNotFound {
        /// The root path that was requested.
        path: String,
    },
    /// The scan root exists but is not a directory.
    #[error("root path is not a directory: {path}")]
    RootNotDirectory {
        /// The root path that was requested.
        path: String,
    },
}

/// The phase of the scan in which a [`TreeIssue`] occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueKind {
    /// A directory could not be listed.
    Walk,
    /// A config file could not be parsed.
    Parse,
}

impl Display for IssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Walk => "walk",
            Self::Parse => "parse",
        })
    }
}

/// One non-fatal problem recorded while scanning a tree.
///
/// Scans keep going: a broken file or an unreadable directory does not
/// abort the walk, it is reported here. The shape is deliberately flat
/// (a kind, a path and a message) so reports stay serialisable as JSON
/// *and* TOML, mirroring the message-carrying style of
/// [`confy_project_meta::MetadataError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeIssue {
    /// Phase in which the issue occurred.
    pub kind: IssueKind,
    /// Path of the unparseable config file or unlistable directory.
    pub path: String,
    /// Display-ready error message.
    pub message: String,
}

impl Display for TreeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            IssueKind::Walk => write!(f, "walk error at {}: {}", self.path, self.message),
            IssueKind::Parse => write!(f, "failed to parse {}: {}", self.path, self.message),
        }
    }
}

/// One config file inside a [`ProjectGroup`], with its metadata when it
/// could be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// The discovered config file.
    pub config: ConfigFile,
    /// The parsed metadata, `None` when parsing failed (see
    /// [`ProjectTree::issues`]).
    pub metadata: Option<ProjectMetadata>,
}

/// The config files of one directory and their metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectGroup {
    /// Directory the config files live in.
    pub dir: PathBuf,
    /// Config files in fixed ecosystem order:
    /// `package.json`, `Cargo.toml`, `deno.json`, `pyproject.toml`.
    pub entries: Vec<ProjectEntry>,
}

/// A directory whose config files disagree with each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMismatch {
    /// Directory containing the disagreeing files.
    pub group_dir: String,
    /// The config file that disagrees with the first-parsed one.
    pub file: String,
    /// Per-field differences.
    pub mismatches: Vec<FieldMismatch>,
}

impl Display for GroupMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "metadata mismatch in {} (group {}): {}",
            self.file,
            self.group_dir,
            self.mismatches
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

/// A package name declared by more than one directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateName {
    /// The repeated package name.
    pub name: String,
    /// Display paths of every declaring config file, sorted.
    pub paths: Vec<String>,
}

impl Display for DuplicateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "duplicate package name `{}` declared by {}",
            self.name,
            self.paths.join(", ")
        )
    }
}

/// The result of scanning a repository tree.
///
/// Groups are ordered lexicographically by directory and entries within
/// a group follow the fixed ecosystem order, so two scans of the same
/// tree always compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectTree {
    /// The root the scan started from.
    pub root: PathBuf,
    /// One group per directory containing config files, sorted by
    /// directory path.
    pub groups: Vec<ProjectGroup>,
    /// Non-fatal problems recorded during the scan, sorted by path.
    pub issues: Vec<TreeIssue>,
}

impl ProjectTree {
    /// Walk `root` and return every config file found.
    ///
    /// Directories named in [`DEFAULT_IGNORED_DIRS`], hidden directories
    /// (unless [`ScanOptions::include_hidden`] is set) and symlinked
    /// directories are never descended into; additional names can be
    /// pruned with [`ScanOptions::with_extra_ignored_dirs`]. Unlistable
    /// directories are skipped silently by this method; use
    /// [`ProjectTree::scan`] to have them reported as issues.
    ///
    /// # Errors
    ///
    /// Returns [`TreeError::RootNotFound`] when `root` does not exist and
    /// [`TreeError::RootNotDirectory`] when it is not a directory.
    pub fn discover(root: &Path, options: &ScanOptions) -> Result<Vec<ConfigFile>, TreeError> {
        Self::check_root(root)?;
        let mut configs = Vec::new();
        let mut issues = Vec::new();
        Self::walk(root, 0, options, &mut configs, &mut issues);
        Ok(configs)
    }

    /// Walk `root`, parse every config file found and group the results
    /// per directory.
    ///
    /// Unlike [`ProjectTree::discover`], parse failures and unlistable
    /// directories do not abort the scan: they are recorded in
    /// [`ProjectTree::issues`] and the affected entry carries `None`
    /// metadata instead.
    ///
    /// # Errors
    ///
    /// Same as [`ProjectTree::discover`].
    pub fn scan(root: &Path, options: &ScanOptions) -> Result<Self, TreeError> {
        Self::check_root(root)?;
        let mut configs = Vec::new();
        let mut issues = Vec::new();
        Self::walk(root, 0, options, &mut configs, &mut issues);

        let mut grouped: BTreeMap<PathBuf, ProjectGroup> = BTreeMap::new();
        for config in configs {
            let dir = config
                .path()
                .parent()
                .map_or_else(PathBuf::new, Path::to_path_buf);
            let metadata = match ProjectMetadata::from_config(&config) {
                Ok(metadata) => Some(metadata),
                Err(error) => {
                    issues.push(TreeIssue {
                        kind: IssueKind::Parse,
                        path: config.path().display().to_string(),
                        message: error.to_string(),
                    });
                    None
                }
            };
            grouped
                .entry(dir.clone())
                .or_insert_with(|| ProjectGroup {
                    dir,
                    entries: Vec::new(),
                })
                .entries
                .push(ProjectEntry { config, metadata });
        }
        let groups = grouped.into_values().collect();
        issues.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self {
            root: root.to_path_buf(),
            groups,
            issues,
        })
    }

    /// Verify that the config files of every directory with at least two
    /// parsed entries agree with each other.
    ///
    /// Every disagreeing group is reported (not just the first one); an
    /// empty result means the whole tree is consistent.
    #[must_use]
    pub fn check_group_consistency(&self) -> Vec<GroupMismatch> {
        let mut mismatches = Vec::new();
        for group in &self.groups {
            let mut parsed = group.entries.iter().filter_map(|entry| {
                entry
                    .metadata
                    .as_ref()
                    .map(|metadata| (&entry.config, metadata))
            });
            let Some((_, first)) = parsed.next() else {
                continue;
            };
            for (config, metadata) in parsed {
                let differences = first.diff(metadata);
                if differences.is_empty() {
                    continue;
                }
                mismatches.push(GroupMismatch {
                    group_dir: group.dir.display().to_string(),
                    file: config.to_string(),
                    mismatches: differences,
                });
            }
        }
        mismatches
    }

    /// Package names declared by more than one directory.
    ///
    /// Repeats *within* a single directory are not reported here (that is
    /// the job of [`ProjectTree::check_group_consistency`]); each
    /// directory contributes at most one occurrence per name. Directories
    /// without a name (e.g. a Cargo workspace root manifest) are skipped.
    #[must_use]
    pub fn duplicate_names(&self) -> Vec<DuplicateName> {
        let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for group in &self.groups {
            let mut seen_in_group = BTreeSet::new();
            for entry in &group.entries {
                let Some(name) = entry
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.name.as_deref())
                else {
                    continue;
                };
                if seen_in_group.insert(name.to_string()) {
                    by_name
                        .entry(name.to_string())
                        .or_default()
                        .insert(entry.config.to_string());
                }
            }
        }
        by_name
            .into_iter()
            .filter(|(_, paths)| paths.len() > 1)
            .map(|(name, paths)| DuplicateName {
                name,
                paths: paths.into_iter().collect(),
            })
            .collect()
    }

    fn check_root(root: &Path) -> Result<(), TreeError> {
        let error = || TreeError::RootNotFound {
            path: root.display().to_string(),
        };
        let metadata = std::fs::metadata(root).map_err(|_| error())?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(TreeError::RootNotDirectory {
                path: root.display().to_string(),
            })
        }
    }

    /// Depth-first walk: the configs of `dir` first, then its qualifying
    /// sub directories in lexicographic order.
    fn walk(
        dir: &Path,
        depth: usize,
        options: &ScanOptions,
        configs: &mut Vec<ConfigFile>,
        issues: &mut Vec<TreeIssue>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                issues.push(TreeIssue {
                    kind: IssueKind::Walk,
                    path: dir.display().to_string(),
                    message: error.to_string(),
                });
                return;
            }
        };
        let mut entries: Vec<(OsString, std::fs::FileType, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                Some((entry.file_name(), file_type, entry.path()))
            })
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));

        // Config files in the fixed ecosystem order, parents before
        // children.
        for kind in [
            ConfigKind::PackageJson,
            ConfigKind::CargoToml,
            ConfigKind::DenoJson,
            ConfigKind::PyprojectToml,
        ] {
            let file_name = OsStr::new(kind.file_name());
            if let Some((_, file_type, path)) =
                entries.iter().find(|(name, _, _)| *name == file_name)
            {
                // Only regular files count; a directory named
                // `package.json` is not a config file. Symlinks are not
                // followed (a symlinked config file is also skipped
                // because `FileType::is_file` does not follow links).
                if file_type.is_file()
                    && let Some(config) = ConfigFile::from_path(path)
                {
                    configs.push(config);
                }
            }
        }

        for (name, file_type, path) in entries {
            // `FileType::is_dir` does not follow symlinks, so symlinked
            // directories are never descended into and the walk cannot
            // cycle.
            if !file_type.is_dir() || options.is_ignored_dir(&name) {
                continue;
            }
            let child_depth = depth + 1;
            if child_depth > options.max_depth {
                continue;
            }
            Self::walk(&path, child_depth, options, configs, issues);
        }
    }
}

#[cfg(test)]
mod tests;
