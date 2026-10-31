<div align="center">
  <img src="docs/logo.png" alt="Confy Logo" width="200"/>
  <p>
    <strong>A modular Rust toolkit that extracts, normalises and cross-checks project metadata across the four major ecosystem config files, written in Rust.</strong><br/>
    Implements a complete metadata pipeline: config file detection (package.json, Cargo.toml, deno.json, pyproject.toml), dependency-spec normalisation across ecosystems, structured field diffing, and typed, feature-gated config structs generated from the official JSON schemas. The project is organised as a Cargo workspace with 3 crates, a unified CLI, enforced 100% test coverage on hand-written code, and a git-cliff driven release pipeline.
  </p>
  <p>
    <img src="https://img.shields.io/badge/Rust-2024%20edition-000000?style=flat-square&logo=rust" alt="Rust"/>
    <img src="https://img.shields.io/badge/License-MIT-blue?style=flat-square" alt="License MIT"/>
    <img src="https://img.shields.io/badge/Crates-3-6F4E37?style=flat-square" alt="Crates"/>
    <img src="https://img.shields.io/badge/Inputs-package.json%20%7C%20Cargo.toml%20%7C%20deno.json%20%7C%20pyproject.toml-CB3837?style=flat-square" alt="Config files"/>
    <img src="https://img.shields.io/badge/CLI-detect%20%2B%20meta%20%2B%20check%20%2B%20schema-4B32C3?style=flat-square" alt="CLI"/>
    <img src="https://img.shields.io/badge/Config%20Types-Schema%20Derived-262D3A?style=flat-square" alt="Typed configs"/>
    <img src="https://img.shields.io/badge/Layered%20Config-confique%20derive-6A0DAD?style=flat-square" alt="confique"/>
    <img src="https://img.shields.io/badge/Diffing-Structured%20Fields-FF4500?style=flat-square" alt="Diffing"/>
    <img src="https://img.shields.io/badge/Testing-Snapshot%20%2B%20Integration-00BFFF?style=flat-square" alt="Testing"/>
    <img src="https://img.shields.io/badge/Coverage-100%25%20lines%20%2B%20functions-brightgreen?style=flat-square" alt="Coverage"/>
    <img src="https://img.shields.io/badge/Lints-Pedantic%20%2B%20Nursery%2C%20zero%20allows-333333?style=flat-square" alt="Lints"/>
    <img src="https://img.shields.io/badge/Changelog-git--cliff-F75C7E?style=flat-square" alt="git-cliff"/>
    <img src="https://img.shields.io/badge/Build-Passing-brightgreen?style=flat-square&logo=githubactions" alt="Build"/>
  </p>
</div>

---

## Why confy

Modern repositories often mix ecosystems — an npm package, a Rust workspace, a Deno module, a Python package — and each declares "the same" project facts in its own dialect: authors as strings or as tables, licenses as SPDX ids or as `{ text = ... }`, dependencies as maps, PEP 508 strings or scheme-prefixed specifiers. **confy** turns all of that into one normalised, comparable model, and gives you the tooling to keep the files in sync.

- **Detect** which config files exist in a directory.
- **Extract** metadata (name, version, description, authors, license, keywords, dependencies, scripts) with ecosystem quirks normalised away.
- **Compare** config files and get structured, per-field mismatches instead of eyeballing diffs.
- **Generate** fully typed Rust structs from the official JSON schemas (via [typify]), optionally deriving [confique]'s layered `Config`.
- **Ship** a single static CLI that bundles all of the above.

## Workspace layout

| Crate | Path | Description |
| --- | --- | --- |
| `confy-project-meta` | [`packages/project-meta`](packages/project-meta) | Core library: `ConfigFile` detection, metadata parsing, dependency normalisation, structured diffing |
| `confy-jschema-configs` | [`packages/jschema-configs`](packages/jschema-configs) | Typed `ConfigRoot` structs code-generated from the package.json / Cargo.toml / deno.json / pyproject.toml JSON schemas |
| `confy-cli` | [`packages/cli`](packages/cli) | The `confy` binary: `detect`, `meta`, `check`, `schema` subcommands |

## Installation

```bash
# From source (Rust 1.88+)
git clone https://github.com/elcoosp/confy
cd confy
cargo install --path packages/cli
```

## CLI usage

```bash
$ confy detect .            # list detected config files
packages/../package.json    # → prints each found path

$ confy check .             # verify all config files agree (exit 1 on mismatch)

$ confy meta . --format json
{
  "configs": [
    {
      "path": "package.json",
      "name": "@elcoosp-configs/biome",
      "version": "0.0.2",
      "description": "Presets configs for biome",
      "authors": ["elcoosp <elcoosp@gmail.com>"],
      "scripts": { "build": "rslib build", "...": "..." }
    }
  ]
}

$ confy schema cargo        # print an embedded ecosystem JSON schema
```

Exit codes: `0` success, `1` metadata mismatch (`check`), `2` error.

## Library usage

```rust
use confy_project_meta::{ConfigFile, ProjectMetadata};

// Parse one config file.
let config = ConfigFile::from_path("Cargo.toml").unwrap();
let metadata = ProjectMetadata::from_config(&config).unwrap();

// Or detect and parse everything found in a directory.
let all = ProjectMetadata::from_detected_config_files(std::path::Path::new(".")).unwrap();

// Structured diffing between two config files.
let mismatches = metadata.diff(&all[0]);
for m in mismatches {
    println!("{m}"); // e.g. `version: 0.2.0 vs 1.0.0`
}

// Enforce cross-file consistency.
ProjectMetadata::check_config_files_equality(std::path::Path::new("."))?;
```

### Normalisation rules

| Field | package.json | Cargo.toml | deno.json | pyproject.toml |
| --- | --- | --- | --- | --- |
| name / version | top-level | `[package]` | top-level | `[project]` |
| authors | `author` string or `authors` array | array of strings | — | `{ name, email }` tables → `Name <email>` |
| license | string | string | string | SPDX string or `{ text }` / `{ file }` |
| keywords | array | array | array | array or comma-separated string |
| dependencies | `dependencies` map | `[dependencies]` (string / table) | `imports` map (`jsr:` / `npm:` / URL) | PEP 508 strings |
| scripts | `scripts` map | — | `tasks` map | — |

Dependency values keep a normalised `spec` plus a [`DependencySource`] (`registry`, `path`, `git`, `url`, `workspace`) so `file:../x`, `{ path = "../x" }` and `@ http://…` all compare meaningfully.

## CI/CD

- **`ci.yml`** — rustfmt, clippy (`pedantic` + `nursery`, `-D warnings`), tests on Linux/macOS/Windows, docs, MSRV check, and a coverage gate that fails below 100% line/function coverage.
- **`release.yml`** — triggered by `v*` tags: [git-cliff] generates release notes from Conventional Commits, release binaries are built for Linux (gnu/musl), macOS (x64/ARM) and Windows, a GitHub release is published with artifacts and changelog, and the three crates are published to crates.io.
- **`cliff.toml`** — git-cliff configuration; **`release.toml`** — [cargo-release] settings (`cargo release minor` bumps, tags and pushes in one command).

### Development

```bash
cargo fmt --all --check          # formatting
cargo clippy --workspace --all-targets   # pedantic + nursery lints (workspace lints)
cargo test --workspace           # unit, doc, snapshot and integration tests
cargo llvm-cov --workspace --all-targets \
  --ignore-filename-regex '.*target/.*build/.*out/.*\.rs' \
  --fail-under-lines 100 --fail-under-functions 100
```

> The `--ignore-filename-regex` excludes typify-generated code from the
> coverage report (it is produced at build time from third-party schemas);
> every line of hand-written code is covered. The `confy-jschema-configs`
> crate waives a small, explicit list of lints in its `Cargo.toml` for the
> same reason — no `#[allow]` attributes exist anywhere in source code.

## License

MIT — see [LICENSE](LICENSE).

[typify]: https://crates.io/crates/typify
[confique]: https://crates.io/crates/confique
[git-cliff]: https://git-cliff.org
[cargo-release]: https://github.com/crate-ci/cargo-release
[`DependencySource`]: packages/project-meta/src/lib.rs
