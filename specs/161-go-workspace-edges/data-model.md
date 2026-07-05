# Data Model: Milestone 161 (Go workspace-mode false dep-graph edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase-1 entity + type inventory. All entities are Rust types in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` unless otherwise noted; wire-shape entities are per-format JSON constructs described in `contracts/annotations.md`.

## Rust types

### E1 — `WorkspaceMode` (NEW enum)

**Location**: NEW type in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceMode {
    /// go.work file present, N use directives parsed successfully.
    /// `use_count == 0` is legal (Q2 clarification 2026-07-04) —
    /// represents an empty-but-valid workspace scaffolding.
    Detected { use_count: usize },
    /// No go.work file at the scanned root, OR GOWORK=off in scan env.
    /// Default variant.
    Absent,
    /// go.work file present but parser rejected it. Reason string
    /// names the failure class (missing-use-close-paren,
    /// invalid-use-path, unknown-directive, etc.).
    Malformed { reason: String },
}

impl Default for WorkspaceMode {
    fn default() -> Self {
        Self::Absent
    }
}

impl WorkspaceMode {
    /// Wire value for `mikebom:go-workspace-mode` (C112).
    /// Per Q2 clarification: empty-use case yields `detected: 0 use-modules`.
    pub fn as_wire_str(&self) -> String {
        match self {
            Self::Detected { use_count } => {
                format!("detected: {use_count} use-modules")
            }
            Self::Absent => "absent".to_string(),
            Self::Malformed { reason } => format!("malformed: {reason}"),
        }
    }

    /// True iff the scan should apply workspace-attribution semantics.
    /// Absent + Malformed both fall through to the milestone-055 non-
    /// workspace resolution path.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Detected { .. })
    }
}
```

**Fields**: enum with 3 variants. **Relationships**: produced by `parse_go_work()`; consumed by the emission code at `scan_cmd.rs` for C112 annotation and by `legacy::read` for edge-attribution branching.

**Validation rules**:
- `Detected { use_count }` — `use_count` MAY be zero per Q2.
- `Malformed { reason }` — reason string MUST be non-empty and MUST identify the parse failure class in a closed-but-extensible vocabulary (see contracts/annotations.md §C112).
- `Absent` — no additional constraints.

### E2 — `GoWorkDocument` (NEW struct)

**Location**: NEW type in `gowork.rs`

```rust
#[derive(Clone, Debug, Default)]
pub struct GoWorkDocument {
    /// The `go X.Y[.Z]` line, if present.
    pub go_version: Option<String>,
    /// `use ( ... )` directive paths — resolved relative to the go.work
    /// file's parent directory. Empty when the file has no use clause
    /// OR when the use block is `use ()`.
    pub use_paths: Vec<PathBuf>,
    /// `replace <old> => <new>` directives. Same shape as the milestone-002
    /// `GoModDocument.replaces` field for reuse of downstream apply logic.
    pub replaces: HashMap<(String, String), (String, String)>,
}
```

**Fields**: 3 parsed sections of the go.work file. **Relationships**: produced by `parse_go_work()`; consumed by `WorkspaceContext.workspace_replaces` (edge-rewrite apply) and by the per-`use`d-module edge-attribution loop in `legacy::read`.

**Validation rules**:
- `use_paths` — each path is stored as normalized relative to the go.work parent dir. Canonicalization via `std::fs::canonicalize` is deferred to `legacy::read` after paths are joined against the workspace root.
- `replaces` — same shape as milestone-002 for compatibility with the existing `apply_replaces` in `graph_resolver.rs:914`.

### E3 — `EdgeDisposition` (NEW enum)

**Location**: NEW type in `gowork.rs` (used by the Q1 hybrid classifier at `legacy::read` post-resolution sweep)

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EdgeDisposition {
    /// Edge target is not workspace-internal or has a resolved version;
    /// keep as-is.
    Keep,
    /// Edge target is workspace-internal AND source's own require block
    /// names the target; rewrite version to the sibling go.mod's declared
    /// version.
    Resolve { sibling_version: String },
    /// Edge target is workspace-internal AND source's own require block
    /// does NOT name the target; drop the edge (FR-002 truthful
    /// attribution).
    Suppress { reason: String },
}
```

**Fields**: 3 variants encoding the Q1 hybrid decision. **Relationships**: produced by `classify_workspace_edge()` (per-edge classifier); consumed by the retention/rewrite pass in `legacy::read`.

**Validation rules**: `Resolve { sibling_version }` MUST carry a non-empty version string. `Suppress { reason }` MUST carry a diagnostic reason for tracing logs.

### E4 — `WorkspaceContext` (EXTENDED, existing struct)

**Location**: `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`

Existing `WorkspaceContext` at `graph_resolver.rs:414` gains 2 new fields:

```rust
pub struct WorkspaceContext {
    // ... existing fields (root_dir, go_sum_modules, replaces, ...) ...

    /// Milestone 161 (E4): workspace-mode detection result. When
    /// `is_active()`, per-`use`d-module edge attribution semantics
    /// apply.
    pub workspace_mode: WorkspaceMode,

    /// Milestone 161 (E4): `use`d module path → filesystem directory
    /// mapping. Empty when workspace_mode.is_active() == false.
    /// Populated by `legacy::read` from GoWorkDocument.use_paths after
    /// each path is canonicalized + its `go.mod` module_path is parsed.
    /// Consumed by the Q1 hybrid classifier.
    pub use_modules_map: HashMap<String, PathBuf>,
}
```

**Relationships**: `workspace_mode` is populated at Go-scan entry by `legacy::read` from `parse_go_work` output. `use_modules_map` is populated after the per-`use`d-module `go.mod` parse pass (needed because each `use`d module's `module` line in its own go.mod yields the canonical module path used as the map key).

### E5 — `ScanDiagnostics.go_workspace_mode` (NEW field)

**Location**: `mikebom-cli/src/scan_fs/package_db/mod.rs:307` (in the existing `ScanDiagnostics` struct — same location as milestone-160's `go_transitive_coverage` field)

```rust
pub struct ScanDiagnostics {
    // ... existing fields ...

    /// Milestone 161 (E5): workspace-mode detection result. Populated
    /// by `read_all` from `GoScanSignals.workspace_mode`. Distinct
    /// from `go_graph_completeness` (C104) and `go_transitive_coverage`
    /// (C110) per research.md R1. `None` iff no Go scan happened
    /// (C112 annotation absent in output).
    pub go_workspace_mode: Option<golang::gowork::WorkspaceMode>,
}
```

Reused by CLI emission at `scan_cmd.rs` (near the existing C110 emission block from milestone 160).

## Wire types

### W1 — `mikebom:go-workspace-mode` (C112, document-scope)

**Wire format**: raw string with grammar `<detection>: <detail>[; ...]`.

Vocabulary:

| Value | Semantic |
|-------|----------|
| `detected: <N> use-modules` | `go.work` file parsed successfully; `<N>` is the count of `use` directive paths. `<N>` MAY be 0 per Q2. |
| `absent` | No `go.work` file at scanned root, OR `GOWORK=off` in scan environment. Absent-annotation preferred; see W1 emission rule below. |
| `malformed: <reason>` | `go.work` file present but parser rejected it. `<reason>` names the failure class. |

**Universality**: Emitted iff `go.work` file present at the scanned root AND parser output was `Detected` or `Malformed`. When `WorkspaceMode::Absent`, the annotation is **entirely absent** from the SBOM (byte-identity guard per SC-003 — a non-workspace scan produces the same wire output pre- and post-161).

**Malformed reason vocabulary** (closed-but-extensible, mirrors milestone-158 C105 governance):

| Reason code | Trigger condition |
|-------------|-------------------|
| `missing-use-close-paren` | `use (` opens a block but no matching `)` before EOF. |
| `invalid-use-path` | A `use` path token is empty, contains invalid UTF-8, or references a non-existent directory. |
| `duplicate-use-path` | Same `use` path appears twice. |
| `unknown-directive` | Line begins with a token that isn't `go`, `use`, `replace`, or comment. |
| `invalid-replace-syntax` | `replace` directive is missing `=>` separator or has malformed sides. |
| `io-error: <detail>` | Filesystem read failure while parsing (e.g. `go.work` file disappears mid-scan). |

**Per-format shape**: see `contracts/annotations.md` §C112.

## Relationships

```text
legacy::read() (Go-scan entry)
     │
     ├── FS check → `<rootfs>/go.work` exists?
     │
     ├── if exists:
     │       parse_go_work() → GoWorkDocument
     │                       → WorkspaceMode
     │                       → use_modules_map (after per-use'd-module go.mod parse)
     │
     ├── if GOWORK=off env var: WorkspaceMode::Absent (override)
     │
     └── produces → WorkspaceContext { workspace_mode, use_modules_map, ... }
                    │
                    ├── consumed by → GraphResolver::resolve()
                    │                  │
                    │                  └── step1_go_mod_graph() invokes `go mod graph`
                    │                      with `GOWORK=off` when workspace_mode.is_active()
                    │                      → each `use`d module's isolated view
                    │
                    ├── consumed by → post-resolution Q1 hybrid sweep in legacy::read
                    │                  │
                    │                  └── classify_workspace_edge() → EdgeDisposition
                    │                      → Keep / Resolve(version) / Suppress(reason)
                    │
                    └── consumed by → GoScanSignals aggregator
                                       │
                                       └── stored in → ScanDiagnostics.go_workspace_mode
                                                       │
                                                       └── emitted by → cli/scan_cmd.rs
                                                                        │
                                                                        └── C112 (doc-scope annotation)
```

## State transitions

**`WorkspaceMode` determination** (per R2 + Q2):

```text
Input: rootfs path, GOWORK env

Step 1 (env override):
  GOWORK == "off"           → WorkspaceMode::Absent

Step 2 (file detection):
  !exists("<rootfs>/go.work") → WorkspaceMode::Absent

Step 3 (parse):
  parse_go_work(text) match:
    Ok(doc)   → WorkspaceMode::Detected { use_count: doc.use_paths.len() }
    Err(e)    → WorkspaceMode::Malformed { reason: classify_parse_err(e) }
```

**Idempotent**: same inputs always produce same output. No hidden state.

## Data volume assumptions

- **Typical `go.work` workspace**: 5–100 `use`d modules. `test-kubernetes` has 47.
- **Q1 hybrid sweep**: O(edges) per project root, ~250 candidate edges total for `test-kubernetes` (47 modules × ~5 direct requires). Sub-second even on the largest Go monorepos observed in practice.
- **Reason-string length**: bounded by fixed prefix vocabulary → ≤120 chars in worst case. No unbounded growth.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| C112 value follows `detected: N use-modules` OR `absent` OR `malformed: <reason>` | Enum-backed at emission (`WorkspaceMode::as_wire_str()`); parity-catalog `Directionality::SymmetricEqual` verifies at test time. |
| C112 emitted iff `go.work` present at scanned root | Guarded by `if diagnostics.go_workspace_mode.is_some()` at emission site AND by the `WorkspaceMode::Absent` case producing `None` (rather than `Some(Absent)`) in the diagnostics population. |
| `malformed:` reason follows the closed-but-extensible 6-code vocab | Enforced by `parse_go_work()` construction; extensions require a spec-milestone bump per milestone-158/160 governance precedent. |
| Q1 hybrid sweep runs iff `WorkspaceMode::is_active()` | Guarded by `if ctx.workspace_mode.is_active() { classify_workspace_edges(...) }` at the post-resolution site. |
| `use_modules_map` is populated exactly when `workspace_mode.is_active()` | Invariant enforced at `legacy::read` construction — `use_modules_map` starts empty in non-workspace scans and receives entries in workspace scans after per-module go.mod parses. |
