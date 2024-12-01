use anyhow::Result;
use confique::Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use thiserror::Error;

#[derive(Serialize, Deserialize, Debug, Config, Eq, PartialEq, Clone)]
pub struct ProjectMetadata {
    name: String,
    version: String,
    description: Option<String>,
    #[serde(alias = "author")]
    authors: Option<Vec<String>>,
    license: Option<String>,
    keywords: Option<Vec<String>>,
    dependencies: Option<Dependencies>,
    scripts: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum Dependencies {
    Simple(HashMap<String, String>),
    Detailed(HashMap<String, DependencyDetails>),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DependencyDetails {
    version: Option<String>,
    url: Option<String>,
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum MetadataError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Failed to read file: {0}")]
    ReadError(String),
    #[error("Failed to parse JSON: {0}")]
    JsonParseError(String),
    #[error("Failed to parse TOML: {0}")]
    TomlParseError(String),
    #[error("No configuration files found")]
    NoFilesFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFile {
    PackageJson(String),
    CargoToml(String),
    DenoJson(String),
    PyprojectToml(String),
}

impl ConfigFile {
    pub fn file_path(&self) -> &str {
        match self {
            ConfigFile::PackageJson(path) => path,
            ConfigFile::CargoToml(path) => path,
            ConfigFile::DenoJson(path) => path,
            ConfigFile::PyprojectToml(path) => path,
        }
    }
}

impl ProjectMetadata {
    pub fn from_config(config_file: &ConfigFile) -> Result<Self, MetadataError> {
        let file_path = config_file.file_path();
        let mut file = File::open(file_path)
            .map_err(|_| MetadataError::FileNotFound(file_path.to_string()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|_| MetadataError::ReadError(file_path.to_string()))?;

        let metadata: ProjectMetadata = match config_file {
            ConfigFile::PackageJson(_) => {
                let json: serde_json::Value = serde_json::from_str(&contents)
                    .map_err(|_| MetadataError::JsonParseError(file_path.to_string()))?;
                let metadata = ProjectMetadata {
                    name: json["name"].as_str().unwrap_or("").to_string(),
                    version: json["version"].as_str().unwrap_or("").to_string(),
                    description: json["description"].as_str().map(|s| s.to_string()),
                    authors: json["author"].as_str().map(|s| vec![s.to_string()]),
                    license: json["license"].as_str().map(|s| s.to_string()),
                    keywords: json["keywords"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                    dependencies: json["dependencies"].as_object().map(|deps| {
                        Dependencies::Simple(HashMap::from_iter(deps.iter().map(|(k, v)| {
                            (k.to_string(), v.as_str().map(|s| s.to_string()).unwrap())
                        })))
                    }),
                    scripts: json["scripts"].as_object().map(|scripts| {
                        scripts
                            .clone()
                            .into_iter()
                            .map(|(k, v)| (k, v.as_str().unwrap_or("").to_string()))
                            .collect()
                    }),
                };
                Ok::<ProjectMetadata, MetadataError>(metadata)
            }
            ConfigFile::CargoToml(_) => {
                let toml: toml::Value = toml::from_str(&contents)
                    .map_err(|_| MetadataError::TomlParseError(file_path.to_string()))?;
                let metadata = ProjectMetadata {
                    name: toml["package"]["name"].as_str().unwrap_or("").to_string(),
                    version: toml["package"]["version"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    description: toml["package"]["description"]
                        .as_str()
                        .map(|s| s.to_string()),
                    authors: toml["package"]["authors"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                    license: toml["package"]["license"].as_str().map(|s| s.to_string()),
                    keywords: toml["package"]["keywords"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                    dependencies: toml["dependencies"].as_table().map(|deps| {
                        let mut detailed_deps = HashMap::new();
                        for (key, value) in deps {
                            let details = DependencyDetails {
                                version: value
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                url: None, // Cargo.toml does not have a URL field
                            };
                            detailed_deps.insert(key.clone(), details);
                        }
                        Dependencies::Detailed(detailed_deps)
                    }),
                    scripts: None, // Cargo.toml does not have a scripts field
                };
                Ok(metadata)
            }
            ConfigFile::DenoJson(_) => {
                let json: serde_json::Value = serde_json::from_str(&contents)
                    .map_err(|_| MetadataError::JsonParseError(file_path.to_string()))?;
                let metadata = ProjectMetadata {
                    name: json["name"].as_str().unwrap_or("").to_string(),
                    version: json["version"].as_str().unwrap_or("").to_string(),
                    description: json["description"].as_str().map(|s| s.to_string()),
                    authors: None, // deno.json does not have an authors field
                    dependencies: json["imports"].as_object().map(|deps| {
                        Dependencies::Simple(HashMap::from_iter(deps.iter().map(|(k, v)| {
                            (k.to_string(), v.as_str().map(|s| s.to_string()).unwrap())
                        })))
                    }),
                    scripts: json["scripts"].as_object().map(|scripts| {
                        scripts
                            .clone()
                            .into_iter()
                            .map(|(k, v)| (k, v.as_str().unwrap_or("").to_string()))
                            .collect()
                    }),
                    license: json["license"].as_str().map(|s| s.to_string()),
                    // FIXME https://github.com/denoland/deno/blob/1d49b3cb0f54eb8184acc00ec4bb3bd519653441/cli/schemas/config-file.v1.json#L4
                    keywords: json["keywords"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                };
                Ok(metadata)
            }
            ConfigFile::PyprojectToml(_) => {
                let toml: toml::Value = toml::from_str(&contents)
                    .map_err(|_| MetadataError::TomlParseError(file_path.to_string()))?;
                let metadata = ProjectMetadata {
                    name: toml["project"]["name"].as_str().unwrap_or("").to_string(),
                    version: toml["project"]["version"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    description: toml["project"]["description"]
                        .as_str()
                        .map(|s| s.to_string()),
                    authors: toml["project"]["authors"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                    license: toml["project"]["license"].as_str().map(|s| s.to_string()),
                    keywords: toml["project"]["keywords"].as_array().map(|arr| {
                        arr.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    }),
                    dependencies: toml["project"]["dependencies"].as_table().map(|deps| {
                        let mut detailed_deps = HashMap::new();
                        for (key, value) in deps {
                            let details = DependencyDetails {
                                version: value
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                url: None, // pyproject.toml does not have a URL field
                            };
                            detailed_deps.insert(key.clone(), details);
                        }
                        Dependencies::Detailed(detailed_deps)
                    }),
                    scripts: None, // pyproject.toml does not have a scripts field
                };
                Ok(metadata)
            }
        }?;

        Ok(metadata)
    }

    pub fn load_from_config_files(
        config_files: Vec<ConfigFile>,
    ) -> Result<Vec<Self>, MetadataError> {
        let mut metadata_list = Vec::new();

        for config_file in config_files {
            match Self::from_config(&config_file) {
                Ok(metadata) => metadata_list.push(metadata),
                Err(MetadataError::FileNotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        if metadata_list.is_empty() {
            return Err(MetadataError::NoFilesFound);
        }

        Ok(metadata_list)
    }
    pub fn from_detected_config_files(cwd: &str) -> Result<Vec<Self>, MetadataError> {
        let config_files = vec![
            ConfigFile::PackageJson(format!("{}/package.json", cwd)),
            ConfigFile::CargoToml(format!("{}/Cargo.toml", cwd)),
            ConfigFile::DenoJson(format!("{}/deno.json", cwd)),
            ConfigFile::PyprojectToml(format!("{}/pyproject.toml", cwd)),
        ];

        Self::load_from_config_files(config_files)
    }
    pub fn check_config_files_equality(cwd: &str) -> Result<(), MetadataError> {
        let config_files = vec![
            ConfigFile::PackageJson(format!("{}/package.json", cwd)),
            ConfigFile::CargoToml(format!("{}/Cargo.toml", cwd)),
            ConfigFile::DenoJson(format!("{}/deno.json", cwd)),
            ConfigFile::PyprojectToml(format!("{}/pyproject.toml", cwd)),
        ];

        let metadata_list = Self::load_from_config_files(config_files.clone())?;

        if metadata_list.is_empty() {
            return Err(MetadataError::NoFilesFound);
        }

        let first_metadata = &metadata_list[0];
        let first_config_path = config_files[0].file_path().to_string();

        for (index, metadata) in metadata_list.iter().enumerate().skip(1) {
            let current_config_path = config_files[index].file_path().to_string();
            if first_metadata != metadata {
                println!(
                    "Difference found between first config file '{}' and config file '{}':",
                    first_config_path, current_config_path
                );
                if first_metadata.name != metadata.name {
                    println!("Name: {} vs {}", first_metadata.name, metadata.name);
                }
                if first_metadata.version != metadata.version {
                    println!(
                        "Version: {} vs {}",
                        first_metadata.version, metadata.version
                    );
                }
                if first_metadata.description != metadata.description {
                    println!(
                        "Description: {:?} vs {:?}",
                        first_metadata.description, metadata.description
                    );
                }
                if first_metadata.authors != metadata.authors {
                    println!(
                        "Authors: {:?} vs {:?}",
                        first_metadata.authors, metadata.authors
                    );
                }
                if first_metadata.license != metadata.license {
                    println!(
                        "License: {:?} vs {:?}",
                        first_metadata.license, metadata.license
                    );
                }
                if first_metadata.keywords != metadata.keywords {
                    println!(
                        "Keywords: {:?} vs {:?}",
                        first_metadata.keywords, metadata.keywords
                    );
                }
                if first_metadata.dependencies != metadata.dependencies {
                    println!(
                        "Dependencies: {:?} vs {:?}",
                        first_metadata.dependencies, metadata.dependencies
                    );
                }
                if first_metadata.scripts != metadata.scripts {
                    println!(
                        "Scripts: {:?} vs {:?}",
                        first_metadata.scripts, metadata.scripts
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::*;
    macro_rules! generate_snapshot_test {
        ($test_name:ident, $config_files:expr) => {
            #[test]
            fn $test_name() {
                let result = ProjectMetadata::load_from_config_files($config_files);
                match result {
                    Err(_) => assert_yaml_snapshot!(result),
                    Ok(ok_result) =>
                    assert_yaml_snapshot!(ok_result, {
                        "[].scripts" => insta::sorted_redaction(),
                        "[].dependencies" => insta::sorted_redaction()
                    })
                }
            }
        };
    }

    // Generate snapshot tests
    generate_snapshot_test!(
        test_happy_path_1,
        vec![ConfigFile::PackageJson("tests/package.json".to_string())]
    );
    generate_snapshot_test!(
        test_happy_path_2,
        vec![ConfigFile::CargoToml("tests/Cargo.toml".to_string())]
    );
    generate_snapshot_test!(
        test_happy_path_3,
        vec![ConfigFile::DenoJson("tests/deno.json".to_string())]
    );
    generate_snapshot_test!(
        test_happy_path_4,
        vec![ConfigFile::PyprojectToml(
            "tests/pyproject.toml".to_string()
        )]
    );
    generate_snapshot_test!(
        test_happy_path_5,
        vec![
            ConfigFile::PackageJson("tests/package.json".to_string()),
            ConfigFile::CargoToml("tests/Cargo.toml".to_string())
        ]
    );
    generate_snapshot_test!(
        test_happy_path_6,
        vec![
            ConfigFile::PackageJson("tests/package.json".to_string()),
            ConfigFile::DenoJson("tests/deno.json".to_string())
        ]
    );
    generate_snapshot_test!(
        test_happy_path_7,
        vec![
            ConfigFile::PackageJson("tests/package.json".to_string()),
            ConfigFile::PyprojectToml("tests/pyproject.toml".to_string())
        ]
    );
    generate_snapshot_test!(
        test_happy_path_8,
        vec![
            ConfigFile::CargoToml("tests/Cargo.toml".to_string()),
            ConfigFile::DenoJson("tests/deno.json".to_string())
        ]
    );
    generate_snapshot_test!(
        test_happy_path_9,
        vec![
            ConfigFile::CargoToml("tests/Cargo.toml".to_string()),
            ConfigFile::PyprojectToml("tests/pyproject.toml".to_string())
        ]
    );
    generate_snapshot_test!(
        test_happy_path_10,
        vec![
            ConfigFile::DenoJson("tests/deno.json".to_string()),
            ConfigFile::PyprojectToml("tests/pyproject.toml".to_string())
        ]
    );

    generate_snapshot_test!(
        test_error_path_1,
        vec![ConfigFile::PackageJson(
            "tests/nonexistent.json".to_string()
        )]
    );
    generate_snapshot_test!(
        test_error_path_2,
        vec![ConfigFile::CargoToml("tests/nonexistent.toml".to_string())]
    );
    generate_snapshot_test!(
        test_error_path_3,
        vec![ConfigFile::DenoJson("tests/nonexistent.json".to_string())]
    );
    generate_snapshot_test!(
        test_error_path_4,
        vec![ConfigFile::PyprojectToml(
            "tests/nonexistent.toml".to_string()
        )]
    );
    generate_snapshot_test!(
        test_error_path_5,
        vec![
            ConfigFile::PackageJson("tests/nonexistent.json".to_string()),
            ConfigFile::CargoToml("tests/nonexistent.toml".to_string())
        ]
    );
}
