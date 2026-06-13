# Contract: Cross-tier auto-alias derivation + `alias_source` provenance

**Feature**: 116-produces-binaries
**Date**: 2026-06-13
**Consumed by**: `verify-binding`, operator inspecting an image-tier SBOM, downstream auditors
**Spec mapping**: FR-002 (case-insensitive + suffix tolerance), FR-003 (audit-trail), FR-004 (operator precedence), FR-013 (collision policy), FR-014 (backwards-compat)

## Pipeline overview

```text
SourceSbomContext::load(path)
  │
  ├─ existing milestone-072 logic:
  │    parse SBOM → component_purls: HashSet<Purl>
  │
  └─ NEW (this feature):
       scan components for mikebom:produces-binaries property
       → populate binary_name_to_purl: HashMap<String, Vec<Purl>>

SourceSbomContext::binding_for_purl(purl) -> SourceDocumentBinding
  │
  ├─ existing milestone-072 logic:
  │    component_purls.contains(purl) → return Verified/Weak/Unknown
  │
  └─ NEW (this feature):
       if Unknown(source-not-found-in-bind-target) AND purl is pkg:generic/<name>:
         lookup_name = normalize(name)  ← case + suffix tolerance
         candidates = binary_name_to_purl.get(lookup_name)
         if candidates.is_empty():
           return original Unknown
         else if candidates.len() == 1:
           return binding for candidates[0] WITH alias_source = AutomaticFromProducesBinaries
         else:
           return Weak(reason: multiple-source-candidates-for-binary-name)
                  WITH alias_source = AutomaticFromProducesBinaries
                  AND audit trail listing all candidates

attach_bindings_to_components(image_components, ctx)
  │
  ├─ if operator --pkg-alias matches: use milestone-111 path
  │    set alias_source = OperatorSupplied
  │    (the auto-alias path is NEVER consulted in this branch)
  │
  └─ else: call ctx.binding_for_purl(image.purl)
       (which may return an auto-alias binding per the NEW branch above)
```

## Image-side normalization (FR-002)

When the binder is checking whether a `pkg:generic/<name>` image-tier PURL has an auto-alias source-tier match, the `<name>` is normalized BEFORE lookup:

1. **Case folding**: `name.to_lowercase()` (ASCII only — non-ASCII names are not expected in practice and not normalized).
2. **Suffix stripping**: if `name` ends in `.exe` (case-insensitive) OR `.jar` (case-insensitive), the suffix is removed. Other suffixes (`.dll`, `.so`, `.dylib`, `.bin`, `.com`, `.bat`, `.ps1`) are NOT stripped — per research.md § Decision 7 those are out of scope.

Examples:

| Image-side PURL `<name>` | Normalized lookup name |
|---|---|
| `baz` | `baz` |
| `Baz` | `baz` |
| `baz.exe` | `baz` |
| `Baz.EXE` | `baz` |
| `baz.jar` | `baz` |
| `baz.tar.gz` | `baz.tar.gz` (not a recognized suffix; passed through) |
| `libbaz.so` | `libbaz.so` (not a recognized suffix; passed through — and `libbaz.so` won't match `baz` in the index, so binding stays `Unknown`) |
| `baz.dll` | `baz.dll` (not stripped — libraries don't bind to source-side binary declarations) |

## Operator precedence rule (FR-004)

When `attach_bindings_to_components()` (`mikebom-cli/src/cli/scan_cmd.rs:2317-2389`) processes an image-tier component:

1. The milestone-111 `--pkg-alias` lookup runs FIRST (at scan_cmd.rs:2337-2343). If the operator declared `pkg:generic/baz=pkg:cargo/baz@1.0.0`, the binder uses `pkg:cargo/baz@1.0.0` as the lookup PURL for `binding_for_purl()`.
2. Inside `binding_for_purl()`, the exact-PURL match against the source SBOM succeeds (because the operator named the correct source-side PURL); the auto-alias branch is NEVER reached.
3. The resulting binding envelope is stamped with `alias_from = pkg:generic/baz`, `alias_to = pkg:cargo/baz@1.0.0`, AND `alias_source = OperatorSupplied`.

The auto-alias path runs ONLY when no operator alias matched the image-tier component's PURL. The two paths cannot both fire for the same component; the precedence is sequential, not racing.

## Audit-trail field (FR-003)

The existing `SourceDocumentBinding` envelope (`mikebom-cli/src/binding/mod.rs:187-217`) gains ONE new field per data-model.md § Entity 2:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub alias_source: Option<AliasSource>,
```

with the new enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AliasSource {
    OperatorSupplied,
    AutomaticFromProducesBinaries,
}
```

Serde renders the variants as `"operator-supplied"` and `"automatic-from-produces-binaries"` in the emitted SBOM.

### Stamping rules

| Binding outcome | `alias_from` / `alias_to` | `alias_source` |
|---|---|---|
| Exact-PURL match (no alias) | None / None | None |
| Operator `--pkg-alias` matched | Set per milestone-111 | `OperatorSupplied` |
| Auto-alias from produces-binaries | Set: from = image PURL, to = source PURL | `AutomaticFromProducesBinaries` |
| Auto-alias with multiple candidates | from = image PURL, to = first candidate | `AutomaticFromProducesBinaries` (binding is `Weak`, reason populated) |

`alias_source` is ALWAYS `Some(_)` when `alias_from` is `Some(_)` (paired-presence invariant) for ALL bindings produced by post-feature mikebom. Pre-feature SBOMs with milestone-111 operator-supplied aliases lack `alias_source`; downstream consumers SHOULD interpret that absence as implicitly `OperatorSupplied` (only source pre-feature).

## Collision policy (FR-013)

When `binary_name_to_purl[lookup_name]` has TWO OR MORE candidates:

- Binding strength = `Weak`
- Reason = `"multiple-source-candidates-for-binary-name"`
- `alias_from` = image PURL, `alias_to` = first candidate (deterministic by source-SBOM document order)
- `alias_source` = `AutomaticFromProducesBinaries`
- **The binding MUST NOT silently pick one candidate at `Verified` strength.** The `Weak` strength + explicit reason signal the ambiguity to consumers.

A future enhancement could add a separate `alias_candidates: Vec<Purl>` field to the envelope listing all candidates; whether to include this in the PR-A initial slice or defer to a follow-up is a review-time call.

## Backwards compatibility (FR-014 / SC-005)

**Pre-feature source SBOMs** (no `mikebom:produces-binaries` property anywhere):
- `binary_name_to_purl` is empty after `load()`
- Every `binding_for_purl()` call short-circuits to the existing exact-PURL match path
- Bindings are byte-identical to pre-feature mikebom output

**Pre-feature image SBOMs** (milestone-072-era, no operator aliases):
- Deserialize cleanly via `#[serde(default)]` on the new `alias_source` field
- Consumers reading the field find `None` — interpret as "no alias was applied"

**Mixed-version round-trip** (milestone-111 SBOMs with operator alias but no `alias_source` field):
- Deserialize cleanly
- The bindings present as `alias_from = Some, alias_to = Some, alias_source = None`
- Consumers reading the field SHOULD interpret as implicit `OperatorSupplied` (the only possible source pre-feature). Post-feature, all newly-emitted bindings populate `alias_source` explicitly.

## Out-of-band overrides

Per research.md § Decision 8, there is NO env-var override or per-scan flag to disable the automatic-alias path. Operators who want exact-PURL-only matching simply don't carry `mikebom:produces-binaries` on their source SBOMs (e.g., by not opting into the source-tier emission, which is gated by the per-ecosystem extractor running). The feature is opt-in at the source-tier emission point.

## Performance contract

- `SourceSbomContext::load()` adds one pass over source-SBOM components scanning their property lists. O(N-components). Expected <10 ms for N=1000.
- `SourceSbomContext::binding_for_purl()` adds at most one `HashMap` lookup + one `to_lowercase()` + one suffix-strip per query for `pkg:generic/<name>` PURLs that miss the exact-match path. O(1) per query. Negligible.
- Total contribution to scan wall time: <50 ms for a typical operator scenario (one source SBOM with ~hundreds of components + one image SBOM with ~tens of `pkg:generic/<name>` binary components).
