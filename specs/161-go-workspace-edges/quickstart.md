# Quickstart: Milestone 161 (Go workspace-mode false dep-graph edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 161. Assumes a working mikebom dev environment (per top-level `CLAUDE.md`).

## 1. Prerequisites

- Rust stable toolchain (workspace-managed).
- The `go` binary on `$PATH` (for T014–T016 empirical investigation AND for the SC-001 audit test).
- Milestone-090 fixture cache populated: `MIKEBOM_FIXTURES_DIR=~/.cache/mikebom/fixtures/<pinned-sha>/` (auto-populated on first test-run).
- **New for m161**: `test-kubernetes` fixture must be added to the fixture-cache repo under `go/workspace-kubernetes/` (see §Fixture setup below).

Verify:

```bash
go version                                    # expect: go1.24+
cargo +stable --version                       # expect: cargo 1.75+
ls -la "$MIKEBOM_FIXTURES_DIR"/go/workspace-kubernetes/go.work  # expect: exists after fixture setup
```

## 2. Fixture setup (test-kubernetes)

A new fixture at `go/workspace-kubernetes/` needs to land in the `kusari-oss/mikebom-fixtures` repo before T014–T016 investigation can begin. Suggested content:

```bash
cd ~/.cache/mikebom/fixtures/<pinned-sha>/go
git clone --depth 1 https://github.com/kubernetes/kubernetes.git workspace-kubernetes
cd workspace-kubernetes
# Trim the fixture size — kubernetes is ~1GB. Keep only go.work, go.sum, and 3-5 staging modules for smoke testing.
```

For the SC-010 integration test, a smaller synthetic fixture at `go/workspace-multi-module/` is sufficient (3 use-modules with a base-library / middle-library / leaf-app dependency shape).

## 3. Investigation loop (FR-007 root causes)

The core of milestone 161 is FR-007's empirical investigation. Iterate on this loop:

```bash
# 1. Baseline: scan test-kubernetes with current mikebom, capture the emitted CDX
cargo build --release --bin mikebom
./target/release/mikebom sbom scan \
    --path "$MIKEBOM_FIXTURES_DIR/go/workspace-kubernetes" \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/mikebom-test-kubernetes.cdx.json

# 2. Extract mikebom's per-workspace-module dependsOn edges
jq '[.dependencies[]
     | {ref, deps: .dependsOn}
     | select(.ref | contains("pkg:golang/k8s.io"))]' \
   /tmp/mikebom-test-kubernetes.cdx.json > /tmp/mikebom-edges.json

# 3. Compute ground truth per use'd module
cd "$MIKEBOM_FIXTURES_DIR/go/workspace-kubernetes"
for use_dir in $(grep -Po '(?<=use )[^\s]+' go.work); do
  echo "=== $use_dir ==="
  (cd "$use_dir" && GOWORK=off go mod graph)
done > /tmp/gomodgraph-per-module.txt

# 4. Diff: which edges are wrong (mikebom-emit ∧ not-in-ground-truth)?
python3 scripts/audit/diff_workspace_edges.py \
    --sbom /tmp/mikebom-test-kubernetes.cdx.json \
    --gomodgraph /tmp/gomodgraph-per-module.txt \
    --show-wrong 20

# 5. Hypothesize the root cause (FR-007a/b/c). Instrument via
#    RUST_LOG=debug + tracing::debug! insertions in legacy.rs.
RUST_LOG=mikebom_cli::scan_fs::package_db::golang=debug \
    ./target/release/mikebom sbom scan --path ... 2>&1 \
    | grep 'workspace' | head -20

# 6. Land the fix. Re-run steps 1-4. Verify wrong-edge reduction.
```

The 3 SC-002 spot-check false edges to suppress:

```text
k8s.io/api → k8s.io/kube-proxy@v0.0.0-unknown          # MUST NOT appear post-161
k8s.io/apimachinery → k8s.io/endpointslice@v0.0.0-unknown  # MUST NOT appear post-161
k8s.io/cli-runtime → k8s.io/streaming@v0.0.0-unknown   # MUST NOT appear post-161
```

## 4. go.work parser implementation

The new parser lives in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`. Structure mirrors the existing `parse_go_mod` at `legacy.rs:200`:

```rust
pub fn parse_go_work(body: &str) -> GoWorkDocument {
    let mut doc = GoWorkDocument::default();
    let mut state = ParserState::Toplevel;
    for (line_no, raw_line) in body.lines().enumerate() {
        let line = strip_comment(raw_line);
        if line.trim().is_empty() {
            continue;
        }
        match state {
            ParserState::Toplevel => {
                if line.starts_with("use ") || line == "use (" {
                    // handle use ... or open block
                } else if line.starts_with("replace ") {
                    // parse single-line replace or open block
                } else if line.starts_with("go ") {
                    doc.go_version = Some(line[3..].trim().to_string());
                }
                // else: ignore unknown top-level (fail-transparent)
            }
            ParserState::InUseBlock => {
                if line.trim() == ")" {
                    state = ParserState::Toplevel;
                } else {
                    // Add use path
                }
            }
            // ... etc
        }
    }
    doc
}
```

Add unit tests inline covering the 6 malformed-reason vocab codes.

## 5. Workspace-attribution implementation

Emission code changes in `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs::read()`:

```rust
// Milestone 161: detect workspace-mode at Go-scan entry
let go_work_path = rootfs.join("go.work");
let workspace_mode = if std::env::var("GOWORK").as_deref() == Ok("off") {
    WorkspaceMode::Absent
} else if go_work_path.is_file() {
    let text = std::fs::read_to_string(&go_work_path)
        .map(|t| parse_go_work(&t))
        .map(|doc| WorkspaceMode::Detected { use_count: doc.use_paths.len() })
        .unwrap_or_else(|e| WorkspaceMode::Malformed { reason: format!("io-error: {e}") });
    text
} else {
    WorkspaceMode::Absent
};

// Populate GoScanSignals for doc-scope emission
if !matches!(workspace_mode, WorkspaceMode::Absent) {
    signals.workspace_mode = Some(workspace_mode.clone());
}

// Per-`use`d-module edge attribution (Q1 hybrid)
for use_dir in &use_paths {
    let ctx = WorkspaceContext {
        // ... existing fields ...
        workspace_mode: workspace_mode.clone(),
        use_modules_map: use_modules_map.clone(),
    };
    // resolver.resolve() picks up workspace_mode.is_active()
    // and passes GOWORK=off to step 1
    let graph_map = resolver.resolve(&ctx, &cache)?;

    // Q1 hybrid disposition sweep
    if ctx.workspace_mode.is_active() {
        classify_workspace_edges(&mut graph_map, &use_modules_map, &sibling_go_mods);
    }

    // Continue with existing entry-building loop
}
```

## 6. Doc-scope annotation emission

Emission goes in `mikebom-cli/src/cli/scan_cmd.rs` near the existing C110 emission (milestone 160). Add a sibling block:

```rust
// Milestone 161: doc-scope go-workspace-mode annotation
if let Some(workspace_mode) = &diagnostics.go_workspace_mode {
    if !matches!(workspace_mode, WorkspaceMode::Absent) {
        add_document_property(
            "mikebom:go-workspace-mode",
            &workspace_mode.as_wire_str(),
        );
    }
}
```

## 7. Parity catalog registration

Update 4 files in one atomic commit:

- `mikebom-cli/src/parity/extractors/cdx.rs` — add `cdx_anno!(c112_cdx, "mikebom:go-workspace-mode", document)`.
- `mikebom-cli/src/parity/extractors/spdx2.rs` — add `spdx23_anno!()` invocation.
- `mikebom-cli/src/parity/extractors/spdx3.rs` — add `spdx3_anno!()` invocation.
- `mikebom-cli/src/parity/extractors/mod.rs` — add 1 `ParityExtractor` registration entry + import the 3 new fn names.

Also update `docs/reference/sbom-format-mapping.md` with the C112 row so the `every_mikebom_emitted_field_has_a_map_row` test passes.

Exact syntax per `contracts/annotations.md` §Parity catalog integration.

## 8. Golden regeneration

**Milestone 161 does NOT change any existing golden.** The existing milestone-090 `golang` fixture is a single-module scan with no `go.work` file → the C112 annotation is absent per SC-003 dual-side byte-identity.

A NEW fixture at `go/workspace-multi-module/` gets its own golden emitted during test runs. Its 3 goldens (CDX + SPDX 2.3 + SPDX 3) will carry the C112 annotation showing `detected: 3 use-modules`.

```bash
# Verify no existing goldens change
for eco in apk bazel cargo cmake deb gem golang maven npm pip rpm; do
    for fmt in cdx.json spdx.json spdx3.jsonld; do
        git diff HEAD -- "mikebom-cli/tests/fixtures/goldens/$eco/scan.$fmt" \
            | wc -l
    done
done
# Expect: 0 diff bytes on all 33 files (including golang).
```

## 9. Test the fix

```bash
# Full pre-PR gate
./scripts/pre-pr.sh

# SC-001 audit (gated behind env var)
MIKEBOM_WORKSPACE_EDGES_AUDIT=1 \
    cargo +stable test --workspace --no-fail-fast \
    --test go_workspace_edges_audit

# Expected: PASS with wrong-edge ratio ≤ 0.05 (from pre-161 baseline of 0.308)
```

## 10. Debugging: tracing recipes

```bash
# See workspace-mode detection outcome
RUST_LOG=mikebom_cli::scan_fs::package_db::golang::gowork=info \
    ./target/release/mikebom sbom scan --path <fixture> 2>&1 \
    | grep 'workspace'

# See per-`use`d-module edge attribution
RUST_LOG=mikebom_cli::scan_fs::package_db::golang::legacy=debug \
    ./target/release/mikebom sbom scan --path <fixture> 2>&1 \
    | grep 'classify_workspace_edge'
```

## 11. Common pitfalls

- **Forgetting to pass `GOWORK=off` to the subprocess**: without this, Go's `go mod graph` returns the merged workspace view instead of the isolated per-module view. Verify via the T032 unit test.
- **Emitting C112 on non-workspace scans**: violates SC-003 byte-identity. Guard emission with `if !matches!(workspace_mode, WorkspaceMode::Absent)`.
- **Confusing workspace-root with `use .` module**: the workspace root's `go.mod` IS a legitimate `use`d module (per FR-003) when `use .` is present. Its edges come from its own require block, not merged into siblings.
- **Q1 hybrid classifier applied globally rather than per-source-module**: the classification is per-EDGE — for each candidate edge, check if the SOURCE's own require block names the target. Different sources may retain/suppress the same target differently.

## 12. Verify SC-002 spot-checks

Post-fix, the 3 concrete false edges MUST NOT appear in the emitted CDX:

```bash
# All three of these should return 0 (no matches)
for false_edge in \
    "k8s.io-api.*k8s.io-kube-proxy" \
    "k8s.io-apimachinery.*k8s.io-endpointslice" \
    "k8s.io-cli-runtime.*k8s.io-streaming"; do
  count=$(jq '.dependencies[]
       | select(.ref | test("k8s.io"))
       | .dependsOn[]
       | select(test("'"$false_edge"'"))' \
       /tmp/mikebom-test-kubernetes.cdx.json | wc -l)
  echo "$false_edge: $count matches (expected 0)"
done
```
