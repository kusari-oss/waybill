# Phase 1 Data Model: C/C++ Ecosystem Expansion (Phase 2)

This document captures the in-process data structures introduced by milestone 105.
Everything is in-memory per scan; no persistent storage; mirrors every
filesystem-scan milestone since 002.

## Domain entities (new)

### `WestManifest`

Parsed shape of a Zephyr `west.yml` file.

```rust
pub struct WestManifest {
    pub defaults: WestDefaults,
    pub remotes: Vec<WestRemote>,
    pub projects: Vec<WestProject>,
    /// `import:` directives — NOT chased transitively in this milestone
    /// per Assumptions. Captured so we can warn that they exist.
    pub imports: Vec<WestImport>,
}

pub struct WestRemote {
    pub name: String,
    pub url_base: String,
}

pub struct WestDefaults {
    pub remote: Option<String>,
}

pub struct WestProject {
    pub name: String,
    pub revision: String,            // SHA or tag; always present per FR-005
    pub remote: Option<String>,      // None → defaults.remote
    pub repo_path: Option<String>,   // explicit override of `<name>` in URL
    pub path: Option<PathBuf>,       // local filesystem checkout (informational)
    pub groups: Vec<String>,         // e.g. ["babblesim", "optional"]
}
```

Validation rules (FR-005):

- `name` MUST be non-empty (warn-and-skip if violated).
- `revision` MUST be present (warn-and-skip if violated).
- If `remote:` is unset, `defaults.remote` MUST resolve to a known `remotes[]` entry; otherwise warn-and-skip.

### `IdfComponentManifest`

Parsed shape of an `idf_component.yml`.

```rust
pub struct IdfComponentManifest {
    pub dependencies: BTreeMap<String, IdfDependency>,
    pub source_path: PathBuf,
}

pub enum IdfDependency {
    Registry { namespace: String, name: String, version: String },
    Local { path: PathBuf },
    Git { url: String, revision: Option<String> },
}
```

A registry dep encodes as `pkg:idf/<namespace>/<name>@<version>` (per FR-006 + clarification Q2).
A local dep encodes as `pkg:generic/<name>` with `mikebom:source-mechanism: "idf-component-local"`.
A git dep encodes as `pkg:git+https://<url>@<rev>` with `mikebom:source-mechanism: "idf-component"`.

### `CpmCallSite`

Extracted from a `cpmaddpackage(...)` / `cpmfindpackage(...)` / `cpmdeclarepackage(...)` call.

```rust
pub struct CpmCallSite {
    pub variant: CpmVariant,             // Add, Find, Declare
    pub name: String,
    pub version: Option<String>,         // VERSION arg
    pub git_tag: Option<String>,         // GIT_TAG arg
    pub github_repository: Option<String>, // GITHUB_REPOSITORY "<org>/<repo>"
    pub git_repository: Option<String>,  // GIT_REPOSITORY full URL
    pub source_file: PathBuf,
}
```

PURL derivation (FR-001):

1. If `github_repository = "org/repo"` is set → `pkg:github/<org>/<repo>@<git_tag or version>`.
2. Else if `git_repository` is set → `pkg:git+https://<url>@<git_tag or version>`. The `<url>` is passed through `sanitize_userinfo` first (FR-016).
3. Else → `pkg:generic/<name>@<version or "unknown">`.

### `ConanRequirement` (extended for `conanfile.py`)

Existing `conanfile.txt` reader emits a `ConanRequirement` shape. The `conanfile.py` extension is additive:

```rust
pub struct ConanRequirement {
    pub name: String,
    pub version: String,
    pub kind: ConanReqKind,
    pub source_file: PathBuf,
}

pub enum ConanReqKind {
    Requires,        // runtime → mikebom:lifecycle-scope: "runtime"
    BuildRequires,   // build  → mikebom:lifecycle-scope: "build"
    ToolRequires,    // tool   → mikebom:lifecycle-scope: "build"
}
```

`conanfile.py` parser uses regex + AST-light heuristics (no Python execution).
Recognized declaration shapes (FR-003):

- `requires = ("name/version", ...)`
- `build_requires = (...)` / `tool_requires = (...)`
- `def requirements(self):` method body containing `self.requires(...)` / `self.tool_requires(...)`
- Inside conditional blocks (`if self.settings.os == "Linux":`), best-effort: emit the component with a `mikebom:lifecycle-scope` annotation reflecting the guard as a string (per spec edge case).

Dynamic computed strings (`self.requires(f"{name}/{version}")`) are skipped with a `tracing::warn!` event naming the file and line.

### `VcpkgClassicInstall`

Per-port install record from `vcpkg/installed/<triplet>/vcpkg/info/<name>_<ver>_<triplet>.list`.

```rust
pub struct VcpkgClassicInstall {
    pub name: String,
    pub version: String,
    pub triplet: String,         // e.g. "x64-linux"
    pub list_path: PathBuf,
}
```

Per FR-007 + edge case: when both classic-mode and manifest-mode declarations exist for the same name, the dedup pipeline (FR-015) chooses manifest-mode as the winner.

### `SubmoduleEntry`

Parsed shape of a `.gitmodules` entry plus the checked-out revision.

```rust
pub struct SubmoduleEntry {
    pub name: String,            // [submodule "<name>"]
    pub path: PathBuf,           // path = ...
    pub url: String,             // url = ...  (sanitized via sanitize_userinfo per FR-016)
    pub head_revision: Option<String>, // resolved from .git/modules/<path>/HEAD
                                       // or .git/modules/<path>/packed-refs;
                                       // None → "unknown" + mikebom:resolver-step annotation per FR-009
}
```

`head_revision` resolution order (no `git` subprocess per the No-subprocesses assumption):

1. `.git/modules/<path>/HEAD` — if it contains a 40-char SHA, that's the revision.
2. If HEAD contains `ref: refs/heads/<branch>`, read `.git/modules/<path>/refs/heads/<branch>`.
3. If neither path resolves, check `.git/modules/<path>/packed-refs` for a matching ref.
4. If still unresolved (uninitialized submodule), `head_revision = None`.

### `DetectionRecord`

Per-reader detection produced during the scan and consumed by the dedup pipeline.

```rust
pub struct DetectionRecord {
    pub canonical_purl: String,
    pub source_mechanism: SourceMechanism,
    pub reader_output: ReaderOutput,    // existing per-reader emission shape
}

pub enum SourceMechanism {
    // existing alpha.41 values
    CmakeFetchcontentGit,
    CmakeFetchcontentUrl,
    CmakeExternalproject,
    CmakeVendored,
    BazelHttpArchive,
    VcpkgManifest,
    ConanRecipe,
    // new in milestone 105
    CpmCmake,
    ZephyrWest,
    IdfComponent,
    IdfComponentLocal,
    VcpkgClassic,
    GitSubmodule,
}
```

The variant order matches the parity-catalog C55 enum ordering documented in `docs/reference/sbom-format-mapping.md`.

### `DedupResult`

Output of the dedup pipeline consumed by SBOM emitters.

```rust
pub struct DedupResult {
    pub winners: Vec<DedupedComponent>,
}

pub struct DedupedComponent {
    pub canonical_purl: String,
    pub winning_source_mechanism: SourceMechanism,
    pub winning_reader_output: ReaderOutput,
    pub also_detected_via: Vec<SourceMechanism>,  // sorted lexicographically; empty if only one reader matched
}
```

The C56 parity row extracts the lexicographically-sorted `also_detected_via` list as a `BTreeSet<String>`.

## Dedup precedence model (FR-015)

Two-stage:

**Stage 1 — Tier precedence** (highest first):

| Tier | Source mechanisms |
|---|---|
| Manifest-mode | `VcpkgManifest`, `VcpkgClassic`, `ConanRecipe`, `CpmCmake`, `ZephyrWest`, `IdfComponent`, `IdfComponentLocal`, `BazelHttpArchive` |
| Filesystem-derived | `GitSubmodule`, `CmakeVendored` |
| Mixed (existing) | `CmakeFetchcontentGit`, `CmakeFetchcontentUrl`, `CmakeExternalproject` |

Note: the existing `CmakeFetchcontent*` and `CmakeExternalproject` values are
manifest-driven (a `CMakeLists.txt` call site) but produce non-canonical PURL
forms (`pkg:generic/<name>@<ver>`). They sit in a third "mixed" tier between
manifest-mode and filesystem-derived — they outrank filesystem-derived but
lose to specific manifest formats (Conan, vcpkg, west, idf, bazel).

**Stage 2 — PURL specificity tie-break** within a tier:

| Rank | PURL prefix |
|---|---|
| 1 (most specific) | `pkg:conan/`, `pkg:vcpkg/`, `pkg:idf/` |
| 2 | `pkg:github/`, `pkg:bazel/` |
| 3 | `pkg:git+https://`, `pkg:git+ssh://` |
| 4 (least specific) | `pkg:generic/` |

**Stage 3 — Deterministic tie-break** if Stages 1+2 don't resolve:
Lexicographic comparison of the `SourceMechanism` discriminant string.
(This is the safety net for the SC-010 determinism test.)

The losing source-mechanism values are sorted lexicographically and emitted in the C56 annotation.

## Annotation entities (new)

### `mikebom:also-detected-via` (C56)

- **Kind**: parity-bridging annotation per Constitution Principle V audit (R1).
- **Shape**: JSON array of source-mechanism strings, sorted lexicographically.
- **Emission**:
  - CDX 1.6: native `evidence.identity[].methods[]` block with one entry per detection record, each carrying a new `mikebom-source-mechanism` sub-field.
  - SPDX 2.3 / 3.0.1: the `mikebom:also-detected-via` annotation as described.
- **Parity row**: C56, `SymmetricEqual`. The CDX extractor reads `evidence.identity[].methods[*].mikebom-source-mechanism` minus the winning value; the SPDX extractors read the annotation directly. Both produce the same `BTreeSet<String>`.

### `mikebom:build-reference` (C57, new — supersedes mis-named `mikebom:linkage-kind` reuse)

- **Kind**: closed-enum annotation.
- **Values**: `"declared-and-used"` | `"declared-only"`.
- **Emission**: only by the `git-submodule` reader (FR-008a). Could be extended to other readers in future milestones, but in scope for 105 only.
- **Parity row**: C57, `SymmetricEqual`.
- **Why a new annotation (not reuse of `mikebom:linkage-kind`)**: see research.md R3 — the existing annotation enforces `dynamic|static|mixed` via a CDX debug-assert.

### `mikebom:source-mechanism` (C55, enum extended)

Existing alpha.41 closed enum:

- `cmake-fetchcontent-git`
- `cmake-fetchcontent-url`
- `cmake-externalproject`
- `cmake-vendored`
- `bazel-http-archive`
- `vcpkg-manifest`
- `conan-recipe`

**Added in milestone 105** (FR-010):

- `cpm-cmake`
- `zephyr-west`
- `idf-component`
- `idf-component-local`
- `vcpkg-classic`
- `git-submodule`

Total: 13 closed-enum values after milestone 105. Documented in `docs/reference/sbom-format-mapping.md`'s C55 row.

## Validation rules summary

| FR | Validation enforced where |
|---|---|
| FR-001 (CPM PURL extraction) | `cpm_cmake::parse_call_site` |
| FR-002 (CPM source-mechanism annotation) | `cpm_cmake::emit` |
| FR-003 (conanfile.py extraction) | `conan::parse_py_recipe` |
| FR-004 (lifecycle-scope tagging) | `conan::emit_py` |
| FR-005 (west.yml extraction) | `west::parse_manifest` |
| FR-006 (idf_component PURL) | `idf_component::emit` |
| FR-007 (vcpkg classic) | `vcpkg::parse_classic_install` |
| FR-008 (submodule PURL + revision lookup) | `git_submodule::parse_and_resolve` |
| FR-008a (build-reference annotation) | `git_submodule::classify_against_find_package_set` |
| FR-009 (uninitialized submodule handling) | `git_submodule::parse_and_resolve` (None branch) |
| FR-010 (enum + catalog) | `parity/extractors/mod.rs` + `docs/reference/sbom-format-mapping.md` |
| FR-011 (additive, no golden break) | enforced by existing parity round-trip test suite |
| FR-012 (offline) | no network calls in any reader (verified by `cargo deny` on `reqwest::Client` usage in the new modules) |
| FR-013 (warn-and-continue) | every reader returns `Vec<PackageDbEntry>` and uses `tracing::warn!` for per-file failures; never returns `Err` from the top-level dispatch path |
| FR-014 (polyglot safety) | the dispatcher (`scan_fs/package_db/mod.rs`) calls each reader in isolation; failures don't propagate |
| FR-015 (dedup precedence) | `scan_fs/dedup.rs` pipeline + SC-010 determinism test |
| FR-016 (credential redaction) | every URL-emitting code path calls `sanitize_userinfo` first; a clippy-lint or grep audit task verifies in CI |
