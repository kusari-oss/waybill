# Data Model — milestone 102

Per-file shape of every deliverable. The milestone has 3 deliverable streams: (a) 4 new readers under `scan_fs/package_db/`, (b) CLI flag plumbing for `--include-vendored`, (c) test fixtures + 12 goldens + docs updates.

## File inventory

| File | State | Owner FRs |
|------|-------|-----------|
| `mikebom-cli/src/scan_fs/package_db/bazel.rs` | NEW | FR-001, FR-002, FR-003, FR-004 |
| `mikebom-cli/src/scan_fs/package_db/cmake.rs` | NEW | FR-005, FR-006, FR-016 |
| `mikebom-cli/src/scan_fs/package_db/vcpkg.rs` | NEW | FR-007 |
| `mikebom-cli/src/scan_fs/package_db/conan.rs` | NEW | FR-008, FR-009 |
| `mikebom-cli/src/scan_fs/package_db/mod.rs` | MODIFY | declare 4 new submodules |
| `mikebom-cli/src/scan_fs/mod.rs` | MODIFY | dispatch 4 new readers + propagate `--include-vendored` |
| `mikebom-cli/src/cli/scan_cmd.rs` | MODIFY | `--include-vendored` CLI flag + plumbing |
| `mikebom-cli/tests/scan_bazel.rs` | NEW | US1 integration test |
| `mikebom-cli/tests/scan_cmake.rs` | NEW | US2 integration test |
| `mikebom-cli/tests/scan_vcpkg.rs` | NEW | US3 vcpkg integration test |
| `mikebom-cli/tests/scan_conan.rs` | NEW | US3 conan integration test |
| `mikebom-cli/tests/scan_cmake_vendored.rs` | NEW | FR-016 `--include-vendored` test |
| `mikebom-cli/tests/cdx_regression.rs` | MODIFY | +4 ecosystem test fns (US1+US2+US3) |
| `mikebom-cli/tests/spdx_regression.rs` | MODIFY | +4 ecosystem test fns |
| `mikebom-cli/tests/spdx3_regression.rs` | MODIFY | +4 ecosystem test fns |
| `mikebom-cli/tests/fixtures/bazel/{MODULE.bazel,WORKSPACE.bazel}` | NEW | Bazel fixture |
| `mikebom-cli/tests/fixtures/cmake/{CMakeLists.txt,cmake/third_party.cmake,third_party/foo/CMakeLists.txt,third_party/foo/version.txt}` | NEW | CMake fixture |
| `mikebom-cli/tests/fixtures/vcpkg/vcpkg.json` | NEW | vcpkg fixture |
| `mikebom-cli/tests/fixtures/conan/{conanfile.txt,conanfile.py}` | NEW | conan fixtures |
| `mikebom-cli/tests/fixtures/golden/cyclonedx/{bazel,cmake,vcpkg,conan}.cdx.json` | NEW (4) | byte-identity goldens |
| `mikebom-cli/tests/fixtures/golden/spdx-2.3/{bazel,cmake,vcpkg,conan}.spdx.json` | NEW (4) | byte-identity goldens |
| `mikebom-cli/tests/fixtures/golden/spdx-3/{bazel,cmake,vcpkg,conan}.spdx3.json` | NEW (4) | byte-identity goldens |
| `README.md` | MODIFY | ecosystems table — add C/C++ Bazel + CMake rows |
| `docs/user-guide/cli-reference.md` | MODIFY | `--include-vendored` flag docs per FR-017 |

Total: 12 NEW source files + 6 MODIFIED + 4 NEW fixture dirs (~8 fixture files) + 12 NEW goldens + 2 MODIFIED docs.

## `bazel.rs` — NEW

```rust
//! Bazel source-tree reader. Parses MODULE.bazel (Bzlmod) +
//! WORKSPACE.bazel (legacy) to extract declared C/C++ dependencies.
//! Per milestone-102 FR-001 .. FR-004.

use std::path::Path;
use regex::Regex;
use mikebom_common::resolution::{LifecycleScope, PackageDbEntry};
use mikebom_common::types::purl::{encode_purl_segment, Purl};
use crate::scan_fs::package_db::common::{ContentHash, ParseErrorAnnotation};

pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    // MODULE.bazel — Bzlmod (preferred per FR-002)
    if let Some(path) = find_file(scan_root, &["MODULE.bazel"]) {
        match parse_module_bazel(&path) {
            Ok(mut deps) => entries.append(&mut deps),
            Err(e) => errors.push(ParseErrorAnnotation { path: path.clone(), error: e.to_string() }),
        }
    }

    // WORKSPACE.bazel + WORKSPACE (legacy, per FR-003)
    for ws_name in &["WORKSPACE.bazel", "WORKSPACE"] {
        if let Some(path) = find_file(scan_root, &[ws_name]) {
            match parse_workspace_bazel(&path) {
                Ok(mut deps) => entries.append(&mut deps),
                Err(e) => errors.push(ParseErrorAnnotation { path: path.clone(), error: e.to_string() }),
            }
        }
    }

    // Dedup MODULE.bazel-vs-WORKSPACE on (name) — MODULE.bazel wins per FR-002 + Edge Cases
    entries = dedup_module_wins(entries);
    (entries, errors)
}

fn parse_module_bazel(path: &Path) -> Result<Vec<PackageDbEntry>, BazelError> {
    let content = std::fs::read_to_string(path)?;
    // Multi-line regex; (?m) for multiline; tolerates whitespace/newlines between args.
    let re = Regex::new(
        r#"(?ms)bazel_dep\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*version\s*=\s*"([^"]+)"(?:\s*,\s*dev_dependency\s*=\s*(True|False))?\s*\)"#,
    )?;
    let path_str = path.to_string_lossy().into_owned();
    Ok(re
        .captures_iter(&content)
        .map(|c| {
            let name = c.get(1).unwrap().as_str();
            let version = c.get(2).unwrap().as_str();
            let dev = c.get(3).map(|m| m.as_str() == "True").unwrap_or(false);
            PackageDbEntry {
                name: name.to_string(),
                version: version.to_string(),
                purl: build_bazel_purl(name, version),
                source_path: path_str.clone(),
                lifecycle_scope: if dev { Some(LifecycleScope::Development) } else { None },
                hashes: vec![],
                extra_annotations: vec![],
                maintainer: None,
                // ... other PackageDbEntry fields default
            }
        })
        .collect())
}

fn parse_workspace_bazel(path: &Path) -> Result<Vec<PackageDbEntry>, BazelError> {
    let content = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    let path_str = path.to_string_lossy().into_owned();

    // http_archive + http_file (per FR-003)
    let http_re = Regex::new(
        r#"(?ms)(http_archive|http_file)\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*urls?\s*=\s*\[?\s*"([^"]+)"\s*\]?\s*(?:,\s*sha256\s*=\s*"([0-9a-fA-F]+)")?\s*[^)]*\)"#,
    )?;
    for c in http_re.captures_iter(&content) {
        let name = c.get(2).unwrap().as_str();
        let url = c.get(3).unwrap().as_str();
        let sha = c.get(4).map(|m| m.as_str().to_string());
        let version = parse_version_from_url(url).unwrap_or_else(|| "unknown".to_string());
        entries.push(PackageDbEntry {
            name: name.to_string(),
            version: version.clone(),
            purl: build_bazel_purl(name, &version),
            source_path: path_str.clone(),
            hashes: sha.map(|s| vec![ContentHash { algorithm: "SHA-256".to_string(), value: s }]).unwrap_or_default(),
            extra_annotations: vec![
                ("mikebom:download-url".to_string(), serde_json::json!(url)),
                ("mikebom:bazel-archive-name".to_string(), serde_json::json!(name)),
            ],
            lifecycle_scope: None,
            ..Default::default()
        });
    }

    // git_repository (per FR-003)
    let git_re = Regex::new(
        r#"(?ms)git_repository\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*remote\s*=\s*"([^"]+)"\s*,\s*(?:commit\s*=\s*"([^"]+)"|tag\s*=\s*"([^"]+)")[^)]*\)"#,
    )?;
    for c in git_re.captures_iter(&content) {
        let name = c.get(1).unwrap().as_str();
        let remote = c.get(2).unwrap().as_str();
        let version = c.get(3).or_else(|| c.get(4)).unwrap().as_str();
        entries.push(PackageDbEntry {
            name: name.to_string(),
            version: version.to_string(),
            purl: build_bazel_purl(name, version),
            source_path: path_str.clone(),
            extra_annotations: vec![
                ("mikebom:download-url".to_string(), serde_json::json!(remote)),
                ("mikebom:bazel-archive-name".to_string(), serde_json::json!(name)),
            ],
            lifecycle_scope: None,
            ..Default::default()
        });
    }
    Ok(entries)
}

fn build_bazel_purl(name: &str, version: &str) -> Purl {
    Purl::new(&format!(
        "pkg:bazel/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version),
    ))
    .expect("constructed PURL is valid")
}

// Helpers: find_file, parse_version_from_url, dedup_module_wins
```

~250-300 lines including tests + docstrings.

## `cmake.rs` — NEW

Same structural shape as `bazel.rs`. Three sub-parsers:

```rust
pub fn read(scan_root: &Path, opts: ReaderOptions) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>) {
    // Walk for CMakeLists.txt + cmake/*.cmake + Modules/*.cmake
    // Per FR-006 + FR-016.
    // ...
}

fn parse_fetchcontent(content: &str, path: &str) -> Vec<PackageDbEntry> {
    let git_re = Regex::new(r#"(?ms)FetchContent_Declare\s*\(\s*(\S+)\s+GIT_REPOSITORY\s+(\S+)\s+GIT_TAG\s+(\S+)[^)]*\)"#)?;
    let url_re = Regex::new(r#"(?ms)FetchContent_Declare\s*\(\s*(\S+)\s+URL\s+(\S+)(?:\s+URL_HASH\s+SHA256=([\dA-Fa-f]+))?[^)]*\)"#)?;
    // GitHub-URL detection for pkg:github/ vs pkg:generic/
    // ...
}

fn parse_externalproject(content: &str, path: &str) -> Vec<PackageDbEntry> { /* same shape */ }

fn parse_add_subdirectory(content: &str, path: &str, opts: &ReaderOptions) -> Vec<PackageDbEntry> {
    if !opts.include_vendored { return vec![]; }
    let re = Regex::new(r#"(?ms)add_subdirectory\s*\(\s*(third_party|vendor)/([^)\s]+)\s*\)"#)?;
    // emit pkg:generic/<name>@<version-from-version.txt> per FR-016
    // ...
}
```

~350 lines including all 3 sub-parsers + tests.

## `vcpkg.rs` — NEW

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct VcpkgManifest {
    #[serde(default)]
    dependencies: Vec<Dependency>,
    #[serde(default)]
    overrides: Vec<Override>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Dependency {
    Simple(String),
    Detailed {
        name: String,
        #[serde(rename = "version>=")]
        version_ge: Option<String>,
        #[serde(default)]
        features: Vec<String>,
    },
}

#[derive(Deserialize)]
struct Override {
    name: String,
    version: String,
}

pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>) {
    let path = scan_root.join("vcpkg.json");
    if !path.exists() { return (vec![], vec![]); }
    match serde_json::from_str::<VcpkgManifest>(&std::fs::read_to_string(&path).unwrap_or_default()) {
        Ok(manifest) => (vcpkg_to_entries(manifest, &path), vec![]),
        Err(e) => (vec![], vec![ParseErrorAnnotation { path, error: e.to_string() }]),
    }
}
```

~150 lines.

## `conan.rs` — NEW

Two sub-parsers per FR-008 + FR-009 — `conanfile.txt` (INI line-by-line) and `conanfile.py` (regex on `requires = [...]`).

```rust
pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    if let Some(path) = find_file(scan_root, &["conanfile.txt"]) {
        match parse_conanfile_txt(&path) {
            Ok(mut deps) => entries.append(&mut deps),
            Err(e) => errors.push(ParseErrorAnnotation { path, error: e.to_string() }),
        }
    }
    if let Some(path) = find_file(scan_root, &["conanfile.py"]) {
        match parse_conanfile_py(&path) {
            Ok(mut deps) => entries.append(&mut deps),
            Err(e) => errors.push(ParseErrorAnnotation { path, error: e.to_string() }),
        }
    }
    (entries, errors)
}
```

~200 lines.

## `package_db/mod.rs` — MODIFY

Declare new submodules:

```rust
// existing modules unchanged
pub mod apk;
pub mod cargo;
// ...
// Milestone 102: C/C++ source-tree readers
pub mod bazel;
pub mod cmake;
pub mod conan;
pub mod vcpkg;
```

## `scan_fs/mod.rs` — MODIFY

Add 4 new reader-dispatch calls in `scan_path()` alongside the existing 11. Roughly:

```rust
pub fn scan_path(root: &Path, ..., include_vendored: bool, ...) -> ScanResult {
    // ... existing readers ...
    let (bazel_entries, mut bazel_errs) = package_db::bazel::read(root);
    let (cmake_entries, mut cmake_errs) = package_db::cmake::read(root, ReaderOptions { include_vendored });
    let (vcpkg_entries, mut vcpkg_errs) = package_db::vcpkg::read(root);
    let (conan_entries, mut conan_errs) = package_db::conan::read(root);
    components.extend(bazel_entries.into_iter().map(to_resolved));
    // ... merge errors into parse_errors vec; surface as scan-summary mikebom:parse-error annotation
}
```

## `cli/scan_cmd.rs` — MODIFY

Add `--include-vendored` per FR-016:

```rust
#[derive(Args)]
pub struct ScanArgs {
    // ... existing fields
    /// Include vendored deps from CMake `add_subdirectory(third_party/...)` (default: off).
    /// See docs/user-guide/cli-reference.md for false-positive risks.
    #[arg(long, env = "MIKEBOM_INCLUDE_VENDORED")]
    pub include_vendored: bool,
}

pub async fn execute(
    args: ScanArgs,
    // ... existing params
    include_vendored: bool,  // NEW
) -> anyhow::Result<()> {
    // pass through to scan_path
}
```

## Test fixtures

### `tests/fixtures/bazel/MODULE.bazel`

```python
module(name = "test_project", version = "0.1.0")
bazel_dep(name = "abseil-cpp", version = "20240722.0")
bazel_dep(name = "googletest", version = "1.14.0", dev_dependency = True)
```

### `tests/fixtures/bazel/WORKSPACE.bazel`

```python
load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
load("@bazel_tools//tools/build_defs/repo:git.bzl", "git_repository")

http_archive(
    name = "rules_python",
    urls = ["https://github.com/bazelbuild/rules_python/archive/0.30.0.tar.gz"],
    sha256 = "abc123...",
)
git_repository(
    name = "rules_foo",
    remote = "https://github.com/foo/rules_foo.git",
    commit = "deadbeef0123",
)
```

### `tests/fixtures/cmake/CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.20)
project(test_project)
include(FetchContent)
include(ExternalProject)

FetchContent_Declare(
    googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG release-1.14.0
)

ExternalProject_Add(
    zlib
    URL https://zlib.net/zlib-1.3.1.tar.gz
    URL_HASH SHA256=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23
)

include(cmake/third_party.cmake)
```

### `tests/fixtures/cmake/cmake/third_party.cmake`

```cmake
FetchContent_Declare(
    boost
    URL https://boostorg.jfrog.io/artifactory/main/release/1.84.0/source/boost_1_84_0.tar.gz
    URL_HASH SHA256=cc4b893acf645c9d4b698e9a0f08ca8846aa5d6c68275c14c3e7949c24109454
)
```

### `tests/fixtures/cmake/third_party/foo/version.txt` (for vendored test)

```
1.2.3
```

### `tests/fixtures/vcpkg/vcpkg.json`

```json
{
  "name": "test-project",
  "version": "0.1.0",
  "dependencies": [
    "zlib",
    { "name": "openssl", "version>=": "3.0.0" }
  ]
}
```

### `tests/fixtures/conan/conanfile.txt`

```ini
[requires]
zlib/1.2.13
openssl/3.0.0

[tool_requires]
cmake/3.27.0
```

### `tests/fixtures/conan/conanfile.py`

```python
from conan import ConanFile

class TestProjectConan(ConanFile):
    name = "test-project"
    version = "0.1.0"
    requires = ["zlib/1.2.13", "openssl/3.0.0"]
    tool_requires = ["cmake/3.27.0"]
```

## Documentation updates

### `README.md` — Ecosystems table

Add 4 rows to the "Supported ecosystems" table covering Bazel, CMake, vcpkg, Conan with the manifest paths each reader picks up.

### `docs/user-guide/cli-reference.md` — `--include-vendored` flag docs

Per FR-017. Must describe: default-OFF behavior, what counts as "vendored" (third_party/ or vendor/ path prefix), false-positive risks (e.g., src/, tests/), and the `version.txt` version-backfill convention.

## Compatibility

- **No `Cargo.lock` change** — pure in-source addition.
- **No production-code change to existing 11 readers** — purely additive.
- **No new crate deps** — regex + toml + serde_json all already in tree.
- **No Linux/macOS/Windows CI behavior change** — readers are cross-platform per FR-013; no `#[cfg]` gates.

## No JSON / no YAML schema additions

Zero new fields in the emission schema. The three new `mikebom:*` properties (`download-url`, `vendored`, `bazel-archive-name`) all use the existing `extra_annotations: Vec<(String, serde_json::Value)>` pattern from milestone 080, which the existing CDX/SPDX 2.3/SPDX 3 emitters already serialize.
