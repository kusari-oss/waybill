# Phase 0 Research: File-tier surfacing for source-heavy trees

**Feature**: 671-file-tier-cpython
**Date**: 2026-09-01
**Status**: Complete

Three clarifications were resolved in the `/speckit.clarify` session (see `spec.md ## Clarifications`). This research fills the remaining unknowns needed before Phase 1 design.

## R1: Existing `FileInventoryMode` enum state

**Decision**: Add a new `SourceTree { restriction: Option<SourceShapeSet> }` variant to the existing enum at `waybill-cli/src/scan_fs/file_tier/mod.rs:292-308`. Update `FileInventoryMode::parse()` at :308 to recognize the new value.

**Rationale**: The enum already has three variants (`Off`, `Orphan`, `Full`) and a hand-rolled `parse()` method that returns `Result<Self, Error>`. Adding a fourth variant with associated data is a minimal-surface change. The restriction is `Option<SourceShapeSet>` — `None` means "all 21 FR-002 shapes"; `Some(set)` means "restrict to this subset per Q1 semantics."

**Alternatives considered**:
- New `SourceTreeInventoryMode` sibling enum → rejected: doubles the mode-check surface at every call site.
- Boolean flag `source_tree: bool` alongside `FileInventoryMode` → rejected: allows nonsensical combos like `Off + source_tree=true` and complicates the CLI-parse layer.

## R2: FR-002 extension allowlist — scoping validation

**Decision**: Lock the FR-002 list to the 21 extensions enumerated in the spec. Extensions are matched case-insensitively against the file's final extension component.

**Extensions**: `.py`, `.pyi`, `.c`, `.cc`, `.cpp`, `.cxx`, `.h`, `.hh`, `.hpp`, `.rs`, `.go`, `.java`, `.kt`, `.js`, `.ts`, `.rb`, `.php`, `.cs`, `.swift`, `.m`, `.mm`

**Rationale**: This list covers the top 20 most-common source-code extensions on GitHub's public-language distribution + `.pyi` (Python stub files, ubiquitous in typed-Python projects). Deliberately excludes:
- **Docs / prose**: `.md`, `.rst`, `.adoc`, `.tex` — often contain user content, not code
- **Configs**: `.toml`, `.yaml`, `.json`, `.ini` — often carry secrets or generated data
- **Build glue**: `Dockerfile`, `Makefile`, `Rakefile` — package-DB readers already claim these
- **Generated**: `.pb.go`, `.pb.py`, `.g.dart` — derivative content

**Cross-check against cpython's real content** (from T014 diagnostic):
- `.py`: 2332 files ✅ (Python source)
- `.c`: ~474 files ✅ (C source)
- `.h`: ~633 files ✅ (C headers)
- Combined: ~3400+ files → meets SC-001 (≥ 100) with ~34× headroom

**Alternatives considered**:
- Broader list including `.md` / `.rst` docs → rejected in Q1 clarification (operators can't add extensions ad-hoc; extension additions require follow-up milestone with proper curation review).
- Narrower list (Python-only) → rejected: excludes cpython's own C source, which is legitimately unattributed content per Principle VIII.

## R3: `SourceShape` enum representation

**Decision**: Define `SourceShape` as a closed enum with one variant per extension (case-normalized). Provide `SourceShape::from_extension(&str) -> Option<Self>` and `SourceShape::as_str(&self) -> &'static str` for round-trip via the annotation channel.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum SourceShape {
    Py, Pyi, C, Cc, Cpp, Cxx, H, Hh, Hpp,
    Rs, Go, Java, Kt, Js, Ts, Rb, Php, Cs,
    Swift, M, Mm,
}
```

**Rationale**: Typed enum enables `SourceShapeSet = BTreeSet<SourceShape>` for the restriction — automatically deduplicates, sorts deterministically, and Rust's exhaustiveness check catches missing arms in `as_str()` if a variant is ever added. Matches Principle IV (type-driven correctness).

`Ord` + `Hash` derived so the set has stable iteration order for the annotation value (SC-007 requires jq-verifiable output).

## R4: FR-009 CLI-parse-fail behavior

**Decision**: `clap`'s `value_parser` closure returns a `thiserror::Error`-derived type on invalid extension. The error message lists all 21 FR-002 extensions verbatim so the operator can immediately see the accepted set.

**Implementation sketch**:

```rust
#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceShapeParseError {
    #[error(
        "unknown source-shape extension {actual:?}; accepted extensions are: \
         py, pyi, c, cc, cpp, cxx, h, hh, hpp, rs, go, java, kt, js, ts, rb, \
         php, cs, swift, m, mm (case-insensitive)"
    )]
    UnknownExtension { actual: String },

    #[error("empty --file-inventory-source-shapes value")]
    Empty,
}

pub(crate) fn parse_restriction(raw: &str)
    -> Result<BTreeSet<SourceShape>, SourceShapeParseError> { ... }
```

**Rationale**: Matches m665's `BinaryScanMode` CLI-parse pattern verbatim. Loud-fail-with-suggestions is the Principle IX (Accuracy) posture — silent acceptance of nonsense values would produce silently-empty file-tier output.

## R5: Companion flag validation (`--file-inventory-source-shapes` outside `--file-inventory=source-tree`)

**Decision**: If the operator passes `--file-inventory-source-shapes=<list>` without also passing `--file-inventory=source-tree`, `clap` fails at parse time with a diagnostic.

**Implementation**: Use `clap`'s `requires` / `conflicts_with` attributes:

```rust
#[arg(long, value_parser = source_shape::parse_restriction, requires = "file_inventory")]
pub file_inventory_source_shapes: Option<BTreeSet<SourceShape>>,
```

Plus a hand-rolled cross-arg check after parse: `if source_shapes.is_some() && !matches!(mode, SourceTree { .. }) { fail with diagnostic }`.

**Rationale**: matches spec FR-001's requirement that "using it under other file-inventory modes MUST fail with a clear diagnostic." Fail-loud reduces silent-misconfiguration risk.

## R6: `content_shape::classify` mode-gated bypass

**Decision**: `classify()` gains a new `mode: FileInventoryMode` parameter (or a smaller `source_tree_active: Option<&SourceShapeSet>` bool-plus-restriction). When the mode is active, the extension-hard-exclusion check at `content_shape.rs:92` becomes conditional:

```rust
// Existing extension-based hard exclusion:
if EXCLUDED_EXTENSIONS.iter().any(|ext| eq_ignore_ascii_case(actual_ext, ext)) {
    // Under SourceTree mode with matching source shape, DON'T skip.
    if let Some(restriction) = source_tree_active {
        if let Some(shape) = SourceShape::from_extension(actual_ext) {
            if restriction_matches(restriction, shape) {
                // Fall through — file qualifies as file-tier candidate under new mode.
                return Some(ContentShape::SourceFile);  // new variant
            }
        }
    }
    return None;
}
```

**Rationale**: Minimal surface change. The existing `EXCLUDED_EXTENSIONS` list (source-code + docs + configs) stays authoritative for the default mode; the mode-gated bypass carves out ONLY the FR-002 source-code extensions (docs + configs still excluded even under source-tree mode).

**Alternatives considered**:
- Add source-shape extensions to a NEW allowlist and require ANY of {orphan-shapes, source-shapes} → rejected: two separate allowlists have overlap-management risk.
- Remove `.py` etc. from `EXCLUDED_EXTENSIONS` unconditionally + rely on the mode-check gate at emission time → rejected: changes default-mode semantics (would trip the FR-007 byte-identity gate).

## R7: New `ContentShape::SourceFile` variant

**Decision**: Add `ContentShape::SourceFile` as a new variant. Emission at the walker keys off this variant to attach a `waybill:file-shape` annotation with the source-shape name (`py`, `c`, etc.) so downstream tools can filter file-tier components by language.

**Rationale**: Existing variants (`ElfBinary`, `PeBinary`, `MachoBinary`, `SharedLib`, `JavaOrArchive`, `OsPackage`, `CompressedArchive`, `LoneManifest`, `ExecScript`) describe binary/artifact shapes. Source files are conceptually distinct and downstream consumers benefit from the discriminator.

**Note on scope**: The `waybill:file-shape` annotation is NOT in the milestone's list of new parity-catalog rows. Deferred to a follow-up because C156 (the mode-activation annotation) is the ONLY new catalog row this milestone needs to close SC-007. Emitting `waybill:file-shape` without a catalog row is safe (only breaks `holistic_parity` if a catalog row exists without an extractor — the reverse is fine per m670 T016 experience).

**Reconsideration**: Actually, if `waybill:file-shape` ships without a catalog row it's less discoverable. Deferring the annotation entirely to a follow-up keeps this milestone tight. The scope-restriction discriminator lives ONLY in the doc-scope C156 annotation for v1.

## R8: Parity-catalog row C156

**Decision**: One new catalog row: `C156 waybill:file-inventory-source-shapes-active` (`SymmetricEqual`, document-scope).

Value shape (JSON-object, JSON-stringified per m670 T012 precedent):
```json
{
  "mode": "source-tree",
  "restriction": ["py", "c", "h"]
}
```

When no restriction is active (`--file-inventory=source-tree` alone, no `--file-inventory-source-shapes`):
```json
{
  "mode": "source-tree",
  "restriction": null
}
```

Annotation is emitted iff and only if the `SourceTree` mode is active. Absent on the default (`--file-inventory=orphan`) path — preserves FR-007 byte-identity.

**Rationale**: Matches m665 C153 `waybill:binary-scan-suppressed` pattern (doc-scope closed-enum for a mode-activation signal) + m670 T012 C154 `waybill:direct-url-source` pattern (JSON-object value with nested null for absent sub-fields).

## R9: Test fixture strategy

**Decision**: Use `tempfile::tempdir()` inline synthetic fixtures per m670 T007 precedent. No new files under `waybill-cli/tests/fixtures/`.

**Fixture shape**:
- 10-20 files: `src/foo.py`, `src/bar.py`, `include/baz.h`, `include/bar.h`, `lib/foo.c`, `lib/bar.c`, plus a few excluded files (`README.md`, `Cargo.toml`, `.gitignore`) to verify the FR-006 skip semantics.
- Test asserts:
  - Default mode → 0 file-tier components (all files are shape-skipped by `EXCLUDED_EXTENSIONS`)
  - SourceTree mode (no restriction) → 6 file-tier components (all .py + .h + .c)
  - SourceTree mode with `.py`-only restriction → 2 file-tier components (only .py)
  - SourceTree mode → doc-scope annotation with correct restriction subset

**Rationale**: Inline fixtures give byte-identical test behavior across hosts + zero cross-repo dependencies + run in every default CI lane. Real-cpython end-to-end verification is a separate task (adhoc sweep or new fixture-corpus entry).

## R10: SC-005 wall-clock envelope

**Decision**: 2× is generous. In practice, SHA-256 over ~3400 additional files @ ~4 KB average = ~14 MB of hashing. On modern hardware, SHA-256 throughput is ~500 MB/s → ~30 ms cost. Walker traversal is the dominant cost, and the walker is single-pass (m664 shared registry) — no re-walk.

**Estimate**: cpython under SourceTree mode ≈ 580 ms baseline + 30-100 ms hashing = 610-680 ms. Well under the 1160 ms (2×) SC-005 ceiling.

**Alternatives considered**: strict 1.5× envelope → rejected as unnecessarily tight given the actual hash cost estimate.

## Summary

Zero remaining `NEEDS CLARIFICATION`. Existing infrastructure at `content_shape.rs` + `walker.rs` accommodates the new mode as an additive variant with mode-gated bypass. New surface: ~150-200 LoC across a new `source_shape.rs` module + minor edits to `content_shape.rs` / `mod.rs` / `scan_cmd.rs` + 1 new catalog row. Ready for Phase 1.
