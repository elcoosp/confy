//! Integration tests running the real `confy` binary.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[test]
fn binary_reports_version_and_help() {
    Command::cargo_bin("confy")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("confy"));
    Command::cargo_bin("confy")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage:"));
}

#[test]
fn binary_detect_meta_check_flow() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "x", "version": "1.0" }"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("deno.json"),
        r#"{ "name": "x", "version": "1.0" }"#,
    )
    .unwrap();

    Command::cargo_bin("confy")
        .unwrap()
        .arg("detect")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains("package.json"));

    Command::cargo_bin("confy")
        .unwrap()
        .arg("meta")
        .arg(dir.path())
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(contains("\"name\": \"x\""));

    Command::cargo_bin("confy")
        .unwrap()
        .arg("check")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains("agree"));

    // A differing file makes `check` exit with code 1 and list mismatches.
    fs::write(
        dir.path().join("deno.json"),
        r#"{ "name": "x", "version": "2.0" }"#,
    )
    .unwrap();
    Command::cargo_bin("confy")
        .unwrap()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("metadata mismatch"));

    // A parse failure is reported with exit code 2.
    fs::write(dir.path().join("deno.json"), "{ nope").unwrap();
    Command::cargo_bin("confy")
        .unwrap()
        .arg("check")
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("error:"));
}

#[test]
fn binary_schema_prints_json() {
    Command::cargo_bin("confy")
        .unwrap()
        .arg("schema")
        .arg("package")
        .assert()
        .success()
        .stdout(contains("JSON schema for NPM package.json files"));
    Command::cargo_bin("confy")
        .unwrap()
        .arg("schema")
        .arg("cargo")
        .assert()
        .success()
        .stdout(contains("TomlManifest"));
}
