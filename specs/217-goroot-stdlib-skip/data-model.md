# Data Model: Skip GOROOT stdlib as Go main-module

**Feature**: 217-goroot-stdlib-skip
**Date**: 2026-07-22

Very small — no new named record types. Two structural changes:

## E1 — Walker-local toolchain observation accumulator

**Location**: `waybill-cli/src/scan_fs/package_db/golang/legacy.rs::candidate_project_roots`.

**Type**: `Vec<PathBuf>` (existing stdlib type; no new type introduced).

**Contents**: absolute or scan-relative paths of DETECTED toolchain roots. A toolchain root is defined as:
- For `module std`: `<go.mod>.parent().parent()` — go.mod is at `$GOROOT/src/go.mod`; toolchain root is `$GOROOT`.
- For `module cmd`: `<go.mod>.parent().parent().parent()` — go.mod is at `$GOROOT/src/cmd/go.mod`; toolchain root is `$GOROOT`.

**Ordering**: sorted lex + deduplicated at accumulator-exit time (before threading to `ScanArtifacts`) so multi-toolchain scans emit deterministic output.

**Lifecycle**: constructed at the top of `candidate_project_roots`, populated inside the walker callback, sorted + deduped, returned via a new tuple return type (or a &mut param passed in — decision at implementation time based on which is least invasive to the walker's callers).

**Threading path**:
```
candidate_project_roots(rootfs, exclude_set) -> (Vec<PathBuf> /* candidates */, Vec<PathBuf> /* toolchain roots */)
    → read()                                — captures the toolchain vec, stores on a new field in DbScanResult or bubbles via a new function param
    → scan_path()                           — stores as a local variable
    → ScanArtifacts.go_toolchains_detected  — Option<&'a [PathBuf]>, None when the vec is empty
    → per-format emitter                    — routes to document-scope annotation slot when Some
```

## E2 — `ScanArtifacts` field addition

**Location**: `waybill-cli/src/generate/mod.rs::ScanArtifacts`.

**New field**:
```rust
/// Milestone 217 (waybill#631): document-scope Go-toolchain-detected
/// signal. `Some(&[<path>, ...])` when the go rootfs walker skipped
/// one or more `module std` / `module cmd` `go.mod` files during scan.
/// The values are toolchain-root paths (parent-of-src, i.e., `$GOROOT`),
/// sorted lex + deduplicated. Consumed by CDX + SPDX 2.3 + SPDX 3
/// document builders and routed to the `waybill:go-toolchain-detected`
/// document-scope annotation (C136). Silent when no toolchain was
/// observed (byte-identity for non-Go and Go-project-only scans).
pub go_toolchains_detected: Option<&'a [PathBuf]>,
```

**Default**: `None`. Every non-Go scan and every user-Go-project-only scan (no toolchain internal go.mod encountered) leaves this as `None` → annotation absent (silence = "not observed").

**Precedent for shape**: matches the m173 `go_cache_warming: Option<&'a CacheWarmingResult>` pattern (borrowed `Option`-wrapped reference on the `'a` lifetime; `None` on non-Go scans).

**Narrow-safe**: matches the m215 `ScanArtifacts::narrow` pattern — the field is a borrowed reference, so it flows through split-mode sub-SBOMs identically to how `go_transitive_coverage` etc. do.

## E3 — Annotation wire shape

**Location**: `waybill:go-toolchain-detected` document-scope annotation.

**CDX 1.6**: `metadata.properties[]` entry:
```json
{
  "name": "waybill:go-toolchain-detected",
  "value": "[\"usr/local/go\",\"opt/homebrew/opt/go/libexec\"]"
}
```

**SPDX 2.3**: document-level `Annotation` on `SPDXRef-DOCUMENT` with `MikebomAnnotationCommentV1` envelope:
```json
{
  "annotationType": "OTHER",
  "annotator": "Tool: waybill",
  "annotationDate": "<created>",
  "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:go-toolchain-detected\",\"value\":\"[\\\"usr/local/go\\\"]\"}"
}
```

**SPDX 3.0.1**: document-scope `Annotation` element on the `SpdxDocument` root IRI; same envelope shape.

**Value format**: JSON-encoded array of strings (paths). Sorted lex, deduplicated. Empty array MUST NOT be emitted — when there are zero detections, the annotation itself is absent (silence = "not observed", matches m176 C121 precedent).

**Path normalization**: paths are scan-root-relative when the toolchain root is under scan_root (typical case); fall back to absolute if outside scan_root (rare — e.g., `--image` scans where the rootfs tempdir is elsewhere; the tempdir prefix is stripped via the same normalization the m176 workspace-paths use).

## E4 — Filter predicate

**Location**: inside `candidate_project_roots`'s `crate::scan_fs::walk::safe_walk` callback, immediately after the `path.is_dir() && path.join("go.mod").is_file()` check.

**Pseudo-code**:
```rust
if path.is_dir() && path.join("go.mod").is_file() {
    let go_mod_path = path.join("go.mod");
    if let Ok(text) = std::fs::read_to_string(&go_mod_path) {
        let doc = parse_go_mod(&text);
        match doc.module_path.as_deref() {
            Some("std") => {
                tracing::debug!(
                    path = %go_mod_path.display(),
                    module = "std",
                    "gorooted-stdlib go.mod skipped (waybill#631)"
                );
                // Toolchain root: $GOROOT/src/go.mod -> $GOROOT
                if let Some(root) = go_mod_path.parent().and_then(|p| p.parent()) {
                    toolchain_roots.push(root.to_path_buf());
                }
                return; // skip — do NOT add to `out`
            }
            Some("cmd") => {
                tracing::debug!(
                    path = %go_mod_path.display(),
                    module = "cmd",
                    "gorooted-cmd go.mod skipped (waybill#631)"
                );
                // Toolchain root: $GOROOT/src/cmd/go.mod -> $GOROOT
                if let Some(root) = go_mod_path.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                    toolchain_roots.push(root.to_path_buf());
                }
                return; // skip
            }
            _ => {}
        }
    }
    // Non-toolchain go.mod: existing behavior — add to `out`.
    out.push(path.to_path_buf());
}
```

## Invariants

- `toolchain_roots` post-sort-dedup has no duplicate entries (typical multi-detection scan: `module std` + `module cmd` in the same `$GOROOT` yields ONE entry).
- `ScanArtifacts.go_toolchains_detected == None` iff `toolchain_roots.is_empty()`.
- The filter never fires on a `go.mod` whose module path is anything other than exact-string `"std"` or `"cmd"` (case-sensitive) → FR-004 backwards-compat guarantee.
- `candidate_project_roots` return type has one new `Vec<PathBuf>` component; existing test call sites of `candidate_project_roots` need updating to accept the tuple. This is a compile-driven refactor caught at build time.

## What we're NOT modeling

- **No new PackageDbEntry variant**: the toolchain observation does NOT become a `pkg:golang/std@...` component. That's the entire point of the fix.
- **No Go version detection**: reading `$GOROOT/VERSION` to construct a synthetic `Go toolchain X.Y.Z` component is explicitly out of scope (spec's Out of Scope section).
- **No propagation to non-golang readers**: the observation is Go-specific; no cross-ecosystem generalization.

## Field additions to existing types

- `waybill-cli/src/generate/mod.rs::ScanArtifacts` — one new `pub go_toolchains_detected: Option<&'a [PathBuf]>` field, `None` default.
- No changes to `waybill-common::resolution::ResolvedComponent` or any other cross-crate type.
