//! Benchmarks for the project-tree scanning pipeline.
//!
//! The tree fixture models a small monorepo: a root directory with all
//! four config files plus a three-level fan-out of directories, each
//! carrying one config file, and `node_modules` / `target` folders full
//! of distractor files that the scanner must prune.

// The `criterion_group!` macro generates a `pub fn benches` that cannot be
// annotated with docs; the workspace `missing_docs` lint is waived for this
// one generated item, mirroring the explicit waiver pattern used by
// `confy-jschema-configs` for its typify-generated code.
#![allow(missing_docs)]

use std::fs;
use std::hint::black_box;
use std::path::Path;

use confy_project_tree::{ProjectTree, ScanOptions};
use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::TempDir;

/// Write one config file of each ecosystem into `dir`.
///
/// Package names collide across directories (`pkg-0` is declared by every
/// directory whose index is a multiple of 8) and a few `deno.json` files
/// are deliberately broken, so the bench exercises the issue and
/// duplicate-name paths of a realistic, slightly messy monorepo.
fn write_configs(dir: &Path, index: usize) {
    let broken = index % 9 == 4;
    if broken {
        fs::write(dir.join("deno.json"), "{ nope").unwrap();
    } else {
        fs::write(
            dir.join("deno.json"),
            format!(
                r#"{{ "name": "pkg-{}", "version": "1.0.{}" }}"#,
                index % 8,
                index
            ),
        )
        .unwrap();
    }
    fs::write(
        dir.join("package.json"),
        format!(
            r#"{{ "name": "pkg-{}", "version": "1.0.{}", "scripts": {{ "build": "echo {}" }} }}"#,
            index % 8,
            index,
            index
        ),
    )
    .unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"crate-{index}\"\nversion = \"1.0.{index}\"\nlicense = \"MIT\"\n\n[dependencies]\nserde = \"1.0\"\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join("pyproject.toml"),
        format!(
            "[project]\nname = \"py-{index}\"\nversion = \"1.0.{index}\"\ndependencies = [\"stringcase~=1.2.0\"]\n"
        ),
    )
    .unwrap();
    // Dependency folders that must be pruned; the nested config files are
    // deliberately invisible to a correct scanner.
    let node_modules = dir.join("node_modules/left-pad");
    fs::create_dir_all(&node_modules).unwrap();
    fs::write(
        node_modules.join("package.json"),
        r#"{ "name": "left-pad" }"#,
    )
    .unwrap();
    let target = dir.join("target/debug");
    fs::create_dir_all(&target).unwrap();
    fs::write(
        target.join("Cargo.toml"),
        "[package]\nname = \"artifact\"\n",
    )
    .unwrap();
}

/// Build a deterministic monorepo fixture: the root plus a three-level
/// fan-out with three children per directory (40 directories, 124 config
/// files, 80 pruned distractor directories).
fn build_tree() -> TempDir {
    let root = TempDir::new().unwrap();
    write_configs(root.path(), 0);
    let mut level = vec![root.path().to_path_buf()];
    for depth in 1..=3 {
        let mut next = Vec::new();
        for (index, parent) in level.iter().enumerate() {
            for child in 0..3 {
                let dir = parent.join(format!("d{depth}-{index}-{child}"));
                fs::create_dir_all(&dir).unwrap();
                write_configs(&dir, depth * 100 + index * 3 + child);
                next.push(dir);
            }
        }
        level = next;
    }
    root
}

fn bench_discover_tree(c: &mut Criterion) {
    let tree = build_tree();
    c.bench_function("discover/40-dir-tree", |b| {
        b.iter(|| {
            ProjectTree::discover(black_box(tree.path()), black_box(&ScanOptions::default()))
                .unwrap()
        });
    });
}

fn bench_scan_tree(c: &mut Criterion) {
    let tree = build_tree();
    c.bench_function("scan/40-dir-tree-124-configs", |b| {
        b.iter(|| {
            ProjectTree::scan(black_box(tree.path()), black_box(&ScanOptions::default())).unwrap()
        });
    });
}

fn bench_check_group_consistency(c: &mut Criterion) {
    let tree = ProjectTree::scan(build_tree().path(), &ScanOptions::default()).unwrap();
    c.bench_function("check-group-consistency/40-groups", |b| {
        b.iter(|| black_box(tree.check_group_consistency()));
    });
}

fn bench_duplicate_names(c: &mut Criterion) {
    let tree = ProjectTree::scan(build_tree().path(), &ScanOptions::default()).unwrap();
    c.bench_function("duplicate-names/40-groups", |b| {
        b.iter(|| black_box(tree.duplicate_names()));
    });
}

criterion_group!(
    benches,
    bench_discover_tree,
    bench_scan_tree,
    bench_check_group_consistency,
    bench_duplicate_names,
);
criterion_main!(benches);
