# Research: Versionless PURL Round-Trip Fuzz Test

**Date**: 2026-07-15
**Purpose**: Resolve 3 investigation-level unknowns before task decomposition — (R1) exactly what round-trip invariant `Purl` guarantees, (R2) what the versionless canonical form looks like per ecosystem, (R3) how to structure the catalog to reach 100+ inputs per ecosystem without ceremony.

## R1 — `Purl` round-trip contract

**Investigation** (grep of `mikebom-common/src/types/purl.rs`):

```rust
pub fn new(raw: &str) -> Result<Self, PurlError> {
    let parsed: packageurl::PackageUrl = raw.parse().map_err(...)?;
    if parsed.name().is_empty() { return Err(PurlError::MissingField("name")); }
    let canonical = canonicalize_qualifiers(raw);
    Ok(Self { canonical, ecosystem: parsed.ty().to_string(), name: ..., version: ..., namespace: ... })
}

pub fn as_str(&self) -> &str { &self.canonical }
```

Key finding: `Purl::as_str()` returns the `canonical` field, which is `canonicalize_qualifiers(raw)` — the input with qualifiers re-sorted lexicographically per purl-spec. The name / version / namespace segments themselves are NOT re-serialized (they pass through unchanged).

**Decision**: The round-trip invariant we assert is `Purl::new(s).as_str() == s` for any input `s` where:
1. `s` parses successfully (Purl::new returns Ok).
2. `s`'s qualifier keys are already sorted lex (or `s` has no qualifiers — the common versionless case).

For the versionless-fuzz case, most catalog entries have NO qualifiers (versionless PURLs typically drop them too). This makes the round-trip check trivially strict — any deviation between input and canonical output is a real bug.

**Alternatives considered**:
- Assert `Purl::new(s).name() == expected_name` etc. (accessor-level check) — weaker than round-trip byte-identity; the fuzz aims to catch every kind of drift.
- Feed randomly-shuffled qualifiers, expect `.as_str()` to return the sorted form — extends scope beyond versionless case. Deferred to a future comprehensive-fuzz milestone.

**References**:
- `mikebom-common/src/types/purl.rs:122-160` — `Purl::new` + accessor definitions.
- `packageurl` crate (workspace dep) — the actual parser.

## R2 — Versionless canonical form per ecosystem

**Purl-spec canonical shape for a versionless PURL**: `pkg:<type>/<namespace>/<name>` — no `@`, no trailing empty version, no qualifiers unless the ecosystem has default namespace-qualifiers.

**Per-ecosystem mikebom-emitted patterns** (as established by m191 primary 5 + m197 US3 6):

| Ecosystem | Type | Versionless shape | Namespace? |
|---|---|---|---|
| npm | `npm` | `pkg:npm/<name>` or `pkg:npm/%40scope/<name>` (scoped) | Optional (`@scope`) |
| cargo | `cargo` | `pkg:cargo/<name>` | No |
| maven | `maven` | `pkg:maven/<groupId>/<artifactId>` | Required (groupId) |
| gem | `gem` | `pkg:gem/<name>` | No |
| pip | `pypi` | `pkg:pypi/<name>` | No |
| composer | `composer` | `pkg:composer/<vendor>/<name>` | Required (vendor) |
| dart | `pub` | `pkg:pub/<name>` | No |
| cocoapods | `cocoapods` | `pkg:cocoapods/<name>` | No |
| scala | `maven` (scala publishes via Maven) | `pkg:maven/<org>/<artifact>` | Required |
| haskell | `hackage` | `pkg:hackage/<name>` | No |
| erlang | `hex` | `pkg:hex/<name>` (or `pkg:hex/<org>/<name>` for private org) | Optional (org) |

**Decision**: The catalog is structured as `(ecosystem_purl_type, [name_shape_variants])` where:
- Ecosystems without namespace (cargo/gem/pypi/pub/cocoapods/hackage/hex): name is the naked package name.
- Ecosystems with namespace (maven, composer): name is `<namespace>/<artifact>`.
- Ecosystems with optional namespace (npm, hex-org): mix — some catalog entries have namespace, some don't.

The test synthesizes PURL strings by simple `format!("pkg:{type}/{name_or_ns_name}")` — no dynamic construction beyond that. Each entry in the catalog is exercised as its versionless canonical form directly.

**Alternatives considered**:
- Generate ecosystem-specific PURLs via each reader's actual `build_*_purl` helper — rejected: couples the fuzz to reader-specific code paths that already have per-reader unit tests. The fuzz is a `Purl` newtype check, not a reader check.

## R3 — Catalog shape + generator strategy

**Decision**: A single `const CATALOG: &[EcosystemFuzz]` where each `EcosystemFuzz` contains an ecosystem-type string + a list of name shape templates. The test loops over `CATALOG × repeats_per_ecosystem`, formats a PURL per (template, ecosystem), and runs the round-trip check.

To reach 100+ inputs per ecosystem without hand-writing 100+ templates:
- Author ~10-15 name-shape templates per ecosystem covering the corner classes:
  - Empty (invalid — tests graceful rejection)
  - Single char (`a`, `x`, `1`)
  - Short common (`foo`, `bar`, `express`)
  - Long realistic (`really-long-package-name-with-hyphens`)
  - Max-length per ecosystem (npm 214 chars, cargo 200, etc.)
  - Unicode (`café`, `名前` — most ecosystems reject; test tolerates rejection)
  - Numeric prefix (`1foo`, `9999` — some ecosystems reject)
  - Hyphen / underscore / dot combos (`foo-bar`, `foo_bar`, `foo.bar`)
  - Percent-encoded (`foo%20bar`, `foo%2Bbar` — verifies canonical preservation)
  - Nested-scope where applicable (`@scope/name`, `com.example:artifact`, `vendor/pkg`)
- Multiply via a suffix rotation: for each template, produce ~10 variants by appending a rotation counter (`foo0`, `foo1`, ..., `foo9`). This crosses the 100-input floor without inflating catalog authorship.

**Total inputs per ecosystem**: ~10-15 templates × ~10 rotations = 100-150 per ecosystem. Across 11 ecosystems = 1100-1650 total invocations. Fits FR-002's ≥ 1100 floor comfortably.

**Alternatives considered**:
- Property-based generation via `proptest::proptest!` — rejected per spec FR-005 zero-new-Cargo-deps constraint. Deterministic catalog is sufficient.
- Fewer templates × more rotations (5 templates × 20 rotations) — rejected: templates ARE where corner-case coverage lives; rotations just multiply. Prefer 10-15 templates.

## R4 — Test binary target creation

**Investigation**: `mikebom-common` currently has no `tests/` sub-directory. Existing tests live inline as `#[cfg(test)] mod tests` blocks in `src/`.

**Decision**: Create `mikebom-common/tests/versionless_purl_fuzz.rs` — cargo automatically discovers this and treats it as an integration-test binary. No `Cargo.toml` edit needed (cargo's `tests/` convention).

The new integration-test binary depends on `mikebom-common` as a normal library dep (import via `use mikebom_common::types::purl::Purl;`). It has NO dev-dep requirements beyond what the library already pulls in — the test uses only stdlib.

**Alternatives considered**:
- Add the fuzz as an inline `#[cfg(test)]` sub-module of `purl.rs` — rejected: `purl.rs` is already dense with per-function unit tests; an inline fuzz would bloat the file. An integration-test binary keeps concerns separate.

## R5 — Diagnostic emission shape

**Decision**: Per-failure diagnostic via `panic!("{}", detail)` (cargo-test convention) with the following structure:

```
purl round-trip drift
  ecosystem:  <purl_type>
  shape:      <catalog_template_name>
  rotation:   <suffix_counter>
  input:      <exact input string>
  observed:   <Purl::as_str() output>
  expected:   <input>  (or <error message> if Purl::new failed unexpectedly)
```

Emitted via `assert_eq!(observed, expected, "purl round-trip drift...\n{block}")` — cargo test's default failure output includes the assertion message with proper indentation.

Per-ecosystem summary at end (verified via SC-002 count check): `println!("[versionless-purl-fuzz] {}: {} invocations", ecosystem, count)`. Emitted regardless of pass/fail so the diagnostic count is always visible.

**Alternatives considered**:
- Custom diagnostic format via `tracing` — rejected: adds a workspace dep to the test surface for no benefit; cargo-test-native `panic!()` is sufficient.

## R6 — Unicode & edge-case tolerance

**Decision**: Every catalog input is tried via `Purl::new(&input)`. If the result is `Err`, the test:
1. Checks whether the input's shape template is marked `expect_reject: true` in the catalog. If yes, silently proceeds (the rejection is deliberate).
2. If `expect_reject: false` (or default), fails with a diagnostic naming ecosystem + shape + the parse error.

For inputs that succeed:
1. Round-trip check: `Purl::new(input).as_str() == input`. Fail with drift diagnostic on mismatch.
2. Accessor checks: `.ecosystem() == expected_ecosystem_type` and `.name() == expected_name`.

This ensures the fuzz tolerates deliberate rejections (unicode-in-cargo, empty-name-anywhere) without generating noise, while catching unexpected rejections + all round-trip drifts.

**Alternatives considered**:
- Blindly ignore ALL parse failures — rejected: hides real regressions.
- Reject-only-empty-inputs — rejected: too permissive; per-ecosystem grammar variance matters.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Round-trip invariant | `Purl::new(s).as_str() == s` for canonical `s` | Accessor-level checks only | Strictest catch-all |
| Catalog shape | `(ecosystem, [templates])` static const | Per-reader helper calls | Isolate Purl-under-test from reader code paths |
| Generator | Templates × rotations = 100+/eco | proptest / quickcheck | Zero new deps per FR-005 |
| Test file location | `mikebom-common/tests/versionless_purl_fuzz.rs` | Inline sub-module in `purl.rs` | Keeps concerns separate |
| Diagnostic shape | `assert_eq!(..., detail_block)` via panic! | Custom `tracing` output | Cargo-test-native; no extra dep |
| Rejection tolerance | Per-input `expect_reject` flag | Blindly ignore parse failures | Deliberate rejections OK, unexpected ones surface |
| Ecosystem-specific quirks | Maven/composer/npm-scoped handled via namespace-in-template | Runtime ecosystem-dispatch | Static catalog is simpler + reviewable |
