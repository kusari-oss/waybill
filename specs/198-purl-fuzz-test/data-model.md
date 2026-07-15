# Data Model: Versionless PURL Round-Trip Fuzz Test

**Date**: 2026-07-15
**Purpose**: Shape of the catalog table + the diagnostic block. Both are test-file-local; no library type changes.

## Entity 1: `EcosystemFuzz` (catalog entry)

**Location**: `mikebom-common/tests/versionless_purl_fuzz.rs`

**Purpose**: One entry per ecosystem in the catalog. The test iterates the catalog and per-ecosystem iterates the name-shape templates × rotations.

```rust
struct EcosystemFuzz {
    /// The `pkg:<type>/...` identifier's type segment.
    /// Examples: "npm", "cargo", "maven", "cocoapods", "hackage".
    ecosystem_type: &'static str,

    /// Whether the ecosystem requires a namespace segment
    /// (e.g., maven groupId, composer vendor). When true, name
    /// templates below MUST embed the namespace as "<ns>/<name>".
    _requires_namespace: bool,

    /// Name-shape templates. Each is a raw name-or-`ns/name` string
    /// used to construct the versionless PURL `pkg:<type>/<name>`.
    /// The rotation counter is appended as a suffix per invocation
    /// to reach the 100+/ecosystem invocation floor.
    templates: &'static [NameShape],
}

struct NameShape {
    /// Human-readable identifier for diagnostic emission (per FR-004).
    label: &'static str,

    /// The name (or `<ns>/<name>` for namespaced ecosystems) template.
    /// The rotation counter is appended without a delimiter.
    template: &'static str,

    /// When true, `Purl::new` is EXPECTED to return Err for this input;
    /// a successful parse from this input would be the actual bug.
    /// Default false. Used for empty-name, unicode-in-rejecting-eco,
    /// max-length-plus-one boundary tests.
    expect_reject: bool,
}
```

**Validation rules**:
- `ecosystem_type` MUST be one of the 11 mikebom-emitted types: `npm`, `cargo`, `maven`, `gem`, `pypi`, `composer`, `pub`, `cocoapods`, `hackage`, `hex`, and (for scala) `maven` (scala publishes via Maven Central so its type IS `maven`; the catalog entry is distinct from the maven entry to have different templates for scala-flavored artifact names).
- `templates` MUST have at least 10 entries per ecosystem to reach ≥ 100 invocations with 10 rotations.
- `expect_reject: true` templates MUST NOT dominate the catalog (majority should be valid inputs — the fuzz's primary purpose is round-trip verification, not rejection verification).

**Initial catalog size target**: 11 ecosystems × ~12 templates × 10 rotations = ~1320 total invocations (comfortably above FR-002's ≥ 1100 floor).

## Entity 2: `FuzzInvocation` (per-iteration record)

Ephemeral — used inside the test loop, not stored between iterations.

```rust
struct FuzzInvocation<'a> {
    ecosystem_type: &'a str,
    shape_label: &'a str,
    rotation: u32,
    input: String,        // format!("pkg:{type}/{template}{rotation}")
}
```

For rotation N with template `foo-bar`, `input` becomes:
- Simple ecosystems: `pkg:npm/foo-bar0` … `pkg:npm/foo-bar9`
- Namespaced ecosystems: `pkg:maven/com.example/foo-bar0` … `pkg:maven/com.example/foo-bar9`
- Scoped npm: `pkg:npm/%40scope/foo-bar0` … `pkg:npm/%40scope/foo-bar9`

## Entity 3: `Diagnostic` (failure emission)

Emitted only on assertion failure. Structured per research §R5 for grep-friendly test output:

```
purl round-trip drift
  ecosystem:  npm
  shape:      short-common
  rotation:   3
  input:      pkg:npm/foo3
  observed:   pkg:npm/foo
  expected:   pkg:npm/foo3
```

Rendered via `assert_eq!(observed, expected, "purl round-trip drift\n  ecosystem: {}\n  shape: {}\n  rotation: {}\n  input: {}", ...)`.

## Cross-cutting: per-ecosystem invocation counter

Static `AtomicUsize` per-ecosystem — incremented per invocation. Printed at test end via `println!("[versionless-purl-fuzz] {}: {}", ecosystem, counter)` for SC-002 verification (≥ 100 per ecosystem visible in test output).

Emitted regardless of pass/fail so operators can spot-check the invocation-count invariant without needing to re-run.

## No new library types

The `Purl` newtype at `mikebom-common/src/types/purl.rs` is unchanged. The catalog + fuzz-loop are the only new code; both live in `mikebom-common/tests/versionless_purl_fuzz.rs`.
