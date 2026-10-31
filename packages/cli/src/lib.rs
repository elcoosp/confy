//! `confy` command line interface: detect project config files, extract and
//! compare their metadata, and print the embedded ecosystem JSON schemas.
//!
//! All logic lives in the library target so it is unit-testable; `main.rs`
//! is a thin wrapper around [`run`].
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use confy_project_meta::{ConfigFile, MetadataError, ProjectMetadata};

/// Embedded JSON schemas shipped with `confy-jschema-configs`.
const SCHEMA_PACKAGE: &str = include_str!("../../jschema-configs/schemas/package.json");
const SCHEMA_CARGO: &str = include_str!("../../jschema-configs/schemas/cargo.json");
const SCHEMA_DENO: &str = include_str!("../../jschema-configs/schemas/deno.json");
const SCHEMA_PYPROJECT: &str = include_str!("../../jschema-configs/schemas/pyproject.json");

/// Output format for the `meta` subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Pretty printed JSON (default).
    Json,
    /// Pretty printed TOML.
    Toml,
}

/// Config kind accepted by the `schema` subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchemaKind {
    /// npm `package.json` schema.
    Package,
    /// Rust `Cargo.toml` schema.
    Cargo,
    /// Deno `deno.json` schema.
    Deno,
    /// Python `pyproject.toml` schema.
    Pyproject,
}

impl SchemaKind {
    const fn schema(self) -> &'static str {
        match self {
            Self::Package => SCHEMA_PACKAGE,
            Self::Cargo => SCHEMA_CARGO,
            Self::Deno => SCHEMA_DENO,
            Self::Pyproject => SCHEMA_PYPROJECT,
        }
    }
}

/// One parsed config file with its path, as rendered by `confy meta`.
#[derive(serde::Serialize)]
struct MetaEntry {
    path: String,
    #[serde(flatten)]
    metadata: ProjectMetadata,
}

/// Root document of `confy meta` output.
#[derive(serde::Serialize)]
struct MetaOutput<'a> {
    configs: &'a [MetaEntry],
}

/// Detect project config files, extract and compare their metadata.
#[derive(Debug, Parser)]
#[command(
    name = "confy",
    version,
    about = "Detect project config files, extract and compare their metadata",
    after_help = "Exit codes: 0 success, 1 metadata mismatch, 2 error"
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Available `confy` subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the config files detected in a directory.
    Detect {
        /// Directory to scan.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Print the normalised metadata of every detected config file.
    Meta {
        /// Directory to scan.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output format.
        #[arg(short, long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Check that every detected config file agrees on its metadata.
    Check {
        /// Directory to scan.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Print one of the embedded ecosystem JSON schemas.
    Schema {
        /// Ecosystem schema to print.
        kind: SchemaKind,
    },
}

/// Execute a parsed [`Cli`], returning the process exit code.
///
/// # Errors
///
/// Returns any I/O or serialisation error encountered while running the
/// selected subcommand.
pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Detect { dir } => detect(&dir),
        Command::Meta { dir, format } => meta(&dir, format),
        Command::Check { dir } => check(&dir),
        Command::Schema { kind } => {
            println!("{}", kind.schema());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn detect(dir: &Path) -> anyhow::Result<ExitCode> {
    let detected = ConfigFile::detect_all(dir);
    if detected.is_empty() {
        return Err(anyhow!("no config files found in {}", dir.display()));
    }
    for config in detected {
        println!("{config}");
    }
    Ok(ExitCode::SUCCESS)
}

fn meta(dir: &Path, format: Format) -> anyhow::Result<ExitCode> {
    let configs = ConfigFile::detect_all(dir);
    if configs.is_empty() {
        return Err(anyhow!("no config files found in {}", dir.display()));
    }
    let entries: Vec<MetaEntry> = configs
        .iter()
        .map(|config| {
            let metadata = ProjectMetadata::from_config(config)
                .with_context(|| format!("failed to parse {}", config.path().display()))?;
            Ok(MetaEntry {
                path: config.to_string(),
                metadata,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MetaOutput { configs: &entries })?
        ),
        Format::Toml => println!(
            "{}",
            toml::to_string_pretty(&MetaOutput { configs: &entries })?
        ),
    }
    Ok(ExitCode::SUCCESS)
}

fn check(dir: &Path) -> anyhow::Result<ExitCode> {
    match ProjectMetadata::check_config_files_equality(dir) {
        Ok(()) => {
            println!("all config files in {} agree", dir.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(MetadataError::NoFilesFound) => {
            Err(anyhow!("no config files found in {}", dir.display()))
        }
        Err(MetadataError::Mismatch { path, mismatches }) => {
            eprintln!("metadata mismatch in {path}:");
            for mismatch in mismatches {
                eprintln!("  - {mismatch}");
            }
            Ok(ExitCode::from(1))
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_pkg(dir: &Path, name: &str, version: &str) {
        fs::write(
            dir.join("package.json"),
            format!(r#"{{ "name": "{name}", "version": "{version}" }}"#),
        )
        .unwrap();
    }

    #[test]
    fn detect_lists_and_errors_on_empty() {
        let dir = tempdir().unwrap();
        write_pkg(dir.path(), "x", "1.0");
        let cli = Cli::try_parse_from(["confy", "detect", dir.path().to_str().unwrap()]).unwrap();
        let output = run(cli).unwrap();
        assert_eq!(output, ExitCode::SUCCESS);

        let empty = tempdir().unwrap();
        let cli = Cli::try_parse_from(["confy", "detect", empty.path().to_str().unwrap()]).unwrap();
        assert!(run(cli).is_err());
    }

    #[test]
    fn meta_prints_json_and_toml() {
        let dir = tempdir().unwrap();
        write_pkg(dir.path(), "x", "1.0");
        for format in ["json", "toml"] {
            let cli = Cli::try_parse_from([
                "confy",
                "meta",
                dir.path().to_str().unwrap(),
                "--format",
                format,
            ])
            .unwrap();
            assert_eq!(run(cli).unwrap(), ExitCode::SUCCESS);
        }
        let empty = tempdir().unwrap();
        let cli = Cli::try_parse_from(["confy", "meta", empty.path().to_str().unwrap()]).unwrap();
        assert!(run(cli).is_err());
    }

    #[test]
    fn meta_reports_parse_errors() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{ nope").unwrap();
        let cli = Cli::try_parse_from(["confy", "meta", dir.path().to_str().unwrap()]).unwrap();
        assert!(run(cli).is_err());
    }

    #[test]
    fn check_exit_codes() {
        let dir = tempdir().unwrap();
        write_pkg(dir.path(), "x", "1.0");
        let cli = Cli::try_parse_from(["confy", "check", dir.path().to_str().unwrap()]).unwrap();
        assert_eq!(run(cli).unwrap(), ExitCode::SUCCESS);

        fs::write(
            dir.path().join("deno.json"),
            r#"{ "name": "x", "version": "2.0" }"#,
        )
        .unwrap();
        let cli = Cli::try_parse_from(["confy", "check", dir.path().to_str().unwrap()]).unwrap();
        assert_eq!(run(cli).unwrap(), ExitCode::from(1));

        let empty = tempdir().unwrap();
        let cli = Cli::try_parse_from(["confy", "check", empty.path().to_str().unwrap()]).unwrap();
        assert!(run(cli).is_err());
    }

    #[test]
    fn check_reports_parse_errors() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{ nope").unwrap();
        let cli = Cli::try_parse_from(["confy", "check", dir.path().to_str().unwrap()]).unwrap();
        assert!(run(cli).is_err());
    }

    #[test]
    fn schema_prints_each_kind() {
        for kind in ["package", "cargo", "deno", "pyproject"] {
            let cli = Cli::try_parse_from(["confy", "schema", kind]).unwrap();
            assert_eq!(run(cli).unwrap(), ExitCode::SUCCESS);
        }
        assert!(Cli::try_parse_from(["confy", "schema", "nope"]).is_err());
    }
}
