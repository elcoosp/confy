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
fn binary_scan_reports_nested_tree() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "root", "version": "1.0" }"#,
    )
    .unwrap();
    let nested = dir.path().join("packages/lib");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("Cargo.toml"),
        "[package]\nname = \"lib\"\nversion = \"1.0\"\n",
    )
    .unwrap();

    // Scan lists both groups in the JSON report.
    Command::cargo_bin("confy")
        .unwrap()
        .arg("scan")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains("packages/lib/Cargo.toml"))
        .stdout(contains("\"root\""));

    // A clean tree passes `scan --check`.
    Command::cargo_bin("confy")
        .unwrap()
        .arg("scan")
        .arg("--check")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains("agree"));

    // A directory disagreeing with itself fails with exit code 1.
    fs::write(
        nested.join("deno.json"),
        r#"{ "name": "lib", "version": "2.0" }"#,
    )
    .unwrap();
    Command::cargo_bin("confy")
        .unwrap()
        .arg("scan")
        .arg("--check")
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("metadata mismatch"));

    // An unknown root is an error (exit code 2).
    Command::cargo_bin("confy")
        .unwrap()
        .arg("scan")
        .arg(dir.path().join("missing"))
        .assert()
        .failure();
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
