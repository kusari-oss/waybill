# Research: Cross-ecosystem dep-name edge resolution

**Feature**: 218-cross-ecosystem-edges | **Date**: 2026-07-22

## R1 — Resolver call-site shape

**Decision**: Extend the existing `(ecosystem, name)`-keyed lookup at `waybill-cli/src/scan_fs/mod.rs:794-810` inline, gated behind the FR-000 flag. Do NOT refactor the resolver into a separate module.

**Rationale**: The failing lookup is a single 5-line block. The full resolver context (the `for (ecosystem, entry) in &packages` loop, the `name_to_purl: HashMap<(String, String), String>` index built at :536, the `Relationship` emission at :799-807) already lives at `mod.rs:794-810`. Threading the new fallback into that block preserves visibility into the same-eco path and lets both paths share the `entry` + `ecosystem` + `purl_str` context that's already in scope. A separate `crate::resolve::name_to_purl::resolve()` refactor was rejected as premature abstraction for a one-site extension (Constitution non-goal).

**Alternatives considered**:
- Split resolver into `waybill-cli/src/resolve/name_to_purl.rs`: rejected — no other call sites need it; would only obscure the change.
- Reuse the m209 `ResolverTrait` chain: rejected — that trait chain is for the deps.dev/purldb enrichment ladder, not for local `name_to_purl` index lookups. Category error.

## R2 — Cross-ecosystem search algorithm

**Decision**: When same-ecosystem lookup at `mod.rs:795-796` returns `None` AND `ecosystem == "generic"`, iterate `name_to_purl.iter()` filtering entries whose key's `.1` equals `normalize_dep_name(candidate_ecosystem, dep_name)` for every candidate ecosystem. Collect matches into a `Vec<(String, String)>` of `(candidate_ecosystem, target_purl)`.

**Rationale**: `HashMap::iter()` over the resolver index is O(N) where N = total component count in the scan. On the fastlane fixture that's ~200 entries × 27 DEPENDENCIES lookups = ~5,400 comparisons per scan. Negligible against I/O-bound scan time. Alternative: build a secondary `HashMap<name, Vec<(ecosystem, purl)>>` sidecar index at initial insertion time (O(1) lookup per name); rejected because the code cost of maintaining the sidecar isn't justified at m216 scale. Can be added in a follow-up if profiling ever shows the linear scan is hot.

**Alternatives considered**:
- Iterate `name_to_purl.keys()` filtering on `.1 == normalized_name`: rejected — normalizes the name once per candidate ecosystem but doesn't materially change the iteration count.
- Build a per-ecosystem `HashMap<String, String>` (name → purl) alongside the current tuple-keyed index: rejected as scope creep — the fix works with the existing shape.

## R3 — Tie-break rule algorithm

**Decision**: FR-003 says "any ecosystem that appears elsewhere in the same scan's non-generic main-modules." Precompute a `HashSet<String>` of non-generic main-module ecosystems ONCE per scan (before the resolver loop starts) by scanning `packages` for entries whose `is_main_module == true` and whose `ecosystem != "generic"`. In the fallback path, intersect the R2 candidate-match set with this precomputed sibling set. If the intersection has exactly one element, emit that single edge (no ambiguity annotation). Otherwise (intersection has zero elements OR multiple elements), emit all candidate-match edges with `waybill:cross-ecosystem-inference-ambiguous` per FR-003.

**Rationale**: Two clarifications drive this: (1) FR-003 explicitly says "emit ALL candidate edges" when tie-break doesn't narrow to exactly one — matches the user's Q1 answer. (2) The precomputation happens once per scan; the intersection is O(candidates × sibling_ecos) which is trivially small (≤10 × ≤10).

**Alternatives considered**:
- Prefer alphabetic-first ecosystem when intersection ≥ 2: rejected — silently picks a single winner, violates the user's Q1 emit-all decision.
- Consider ONLY non-generic main-modules from the same `source_path` directory subtree: rejected as over-narrow — polyglot repos may have their Rails main-module and their pip helper in unrelated directories.

## R4 — Annotation payload shape + canonicalization

**Decision**: `waybill:cross-ecosystem-inference` value is a JSON object serialized via `serde_json::to_string_pretty(...)` → NO, `serde_json::to_string(...)` (compact form) with a `#[derive(Serialize)]` struct whose fields are declared in alphabetic order: `from_eco`, `lookup_via`, `target_purl`, `to_eco`. `serde_json` emits fields in struct-declaration order, so declaring them alphabetically produces canonical output without additional sort logic. Byte-identity is trivially achieved.

**Rationale**: All three formats round-trip the annotation value as an opaque string; canonical bytes make byte-identity assertions across scan runs (and across parity extractors) deterministic. Matches m134 divergence-record precedent (`serde_json::to_string` on a struct with fixed field order).

**Payload shape** (per Q3 clarification):

```rust
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct CrossEcosystemInferencePayload {
    pub from_eco: String,      // e.g. "generic"
    pub lookup_via: String,    // e.g. "gemfile-lock-dependencies"
    pub target_purl: String,   // e.g. "pkg:gem/fastlane@2.220.0"
    pub to_eco: String,        // e.g. "gem"
}
```

**Ambiguous variant** (`waybill:cross-ecosystem-inference-ambiguous`): extends the base with a fifth field `alternates: Vec<AlternateMatch>` where `AlternateMatch = { target_purl, to_eco }`. Sorted lex by `target_purl` for byte-identity.

**Alternatives considered**:
- Serialize with `serde_json::json!({...})` + `Value.to_string()`: rejected — `serde_json::Value` object ordering is non-canonical without `Value::preserve_order` feature; the struct-based path is simpler and doesn't require a feature bump.
- Bincode: rejected — annotation values MUST be strings per parity contract; consumers parse them as JSON.

## R5 — Per-format landing slot exact wire shape

**CycloneDX 1.6** — `dependencies[i].properties[]` supports `[{name, value}]` per the CDX 1.6 JSON schema. Property emission at existing `waybill-cli/src/generate/cyclonedx/dependencies.rs` (search for `dependencies[i]` construction — the property array is currently absent). Per-edge property looks like `{"name": "waybill:cross-ecosystem-inference", "value": "<canonical-json-string>"}`. CDX doesn't have per-target-within-dependsOn granularity, so when a source has 5 targets and 3 are crossed, the source's `properties[]` gets 3 property objects (one per crossed target), each with `target_purl` in its payload disambiguating which target the property applies to.

**SPDX 2.3** — `Package.annotations[]` on the source Package (the from-side of the DEPENDS_ON relationship). Envelope: standard `MikebomAnnotationCommentV1` (already used by every other `waybill:*` annotation). Payload identifies the target via the embedded `target_purl` field — same disambiguation strategy as CDX. SPDX 2.3 has no relationship-level annotation slot; the source-Package-scoped annotation with in-payload target-PURL is the standards-native landing.

**SPDX 3** — `Annotation` element whose `subject` IRI is the specific `Relationship` element joining source + target. This is the truest per-edge shape (SPDX 3 supports annotations on any Element IRI including Relationships). Emission slot: existing `waybill-cli/src/generate/spdx/relationships.rs` — after each Relationship emission, if that relationship is a cross-ecosystem edge, append an Annotation to the document body.

**Doc-scope C139 (`waybill:cross-ecosystem-inference-unresolved`)** — same slots as m217 C136 doc-scope Go-toolchain-detected: CDX `metadata.properties[]`, SPDX 2.3 doc-level `Annotation` on SPDXRef-DOCUMENT, SPDX 3 `Annotation` on the SpdxDocument root IRI.

## R6 — Documentation deliverable structure

**Decision**: Author `docs/reference/cross-ecosystem-edges.md` covering the five FR-014 required sections:

1. **What the flag does** (~15 lines): one paragraph explaining resolver-behavior delta, one paragraph explaining the m216 gap that motivated it.
2. **When to enable it** (~10 lines): one bullet per current use-case (Gemfile-only Ruby apps today), one paragraph noting automatic inheritance by future m216-alike readers.
3. **Interpreting the three annotations** (~60 lines): sub-sections for each of `cross-ecosystem-inference`, `cross-ecosystem-inference-ambiguous`, `cross-ecosystem-inference-unresolved`. Each sub-section has: JSON payload example, per-format landing-slot table (CDX / SPDX 2.3 / SPDX 3), one worked "how a consumer parses this" code snippet (Python via `json.loads`, TypeScript via `JSON.parse`).
4. **Decision tree for consumers** (~40 lines): ASCII flow-chart showing "does the edge carry `-inference`?" → "does it carry `-ambiguous`?" → "does the doc carry `-unresolved`?" → recommended action for each terminal state (trust, prefer verify, treat as gap).
5. **Experimental status disclaimer** (~10 lines): what "experimental" means for this flag, what graduation looks like, how consumers should pin the flag if they've already adopted it.
6. **Worked example** (~50 lines): full flag-on scan output snippet for a mini Gemfile fixture, showing one same-eco edge (no annotation), one cross-eco resolved edge (with annotation), and one unresolved-name doc annotation. Consumer JSON extraction shown alongside.

Total ~200 lines. Linked from `README.md`'s "SBOM interpretation" section (add if absent) and from `docs/reference/sbom-format-mapping.md` C137/C138/C139 rows.

**Rationale**: FR-014 enumerates the required topics. The 200-line target matches the shape of existing docs like `docs/reference/component-tiers.md` (Constitution §Completeness reference) and `docs/reference/sbom-format-mapping.md` (parity catalog).

## R7 — Fastlane baseline math

**Decision**: Post-m216 baseline is `EXPECTED_WAYBILL_EDGE_COUNT: usize = 197` at `waybill-cli/tests/transitive_parity_gem.rs:45`. The fastlane fixture's `Gemfile.lock` DEPENDENCIES block has **27 entries** (counted via `sed -n '/^DEPENDENCIES/,/^$/p' Gemfile.lock | wc -l → 28 lines including the header, so 27 gem lines`).

Of those 27, some will not resolve:
- `fastlane!` — the `!` suffix marks a git-sourced gem; the m216 emitter strips this. The name `fastlane` IS in the GEM section (fastlane 2.220.0), so it resolves.
- `fastlane-plugin-clubmate`, `fastlane-plugin-ruby`, `fastlane-plugin-slack_train` — plugins may or may not appear in the GEM section. Grep says they don't (only `fastlane-plugin-slack_train` appears at the plugin gem-listing level; the other two need to be verified at implementation time).

**Conservative flag-on baseline**: `197 + N_resolved` where `24 ≤ N_resolved ≤ 27`. The test asserts a lower bound of `197 + 24 = 221` and an upper bound of `197 + 27 = 224`. Implementation asserts the exact value once the resolver is running against the fixture; the range is the guardrail.

**Rationale**: Locking to a single exact number requires running the resolver first; the range prevents flakes from plugin-availability variance without giving up SC-002 measurability.

## R8 — Test isolation from existing m216-only assertions

**Decision**: The existing `transitive_parity_gem.rs` test asserts the 197-edge baseline WITHOUT the FR-000 flag. Extend that test file with a new `#[test] fn m218_flag_on_recovers_edges_from_pkg_generic_main_module()` that runs the same fixture WITH the FR-000 flag and asserts `≥ 221` edges + presence of C137 annotations on the recovered edges. Do NOT modify the existing 197-edge test — SC-009 (byte-identity when flag OFF) explicitly requires the old assertion to keep passing verbatim.

**Rationale**: Separating flag-off from flag-on assertions into two distinct `#[test]` functions makes the SC-009 gate mechanical (grep for `EXPECTED_WAYBILL_EDGE_COUNT` — value unchanged, comment updated with a new line about the m218 flag-on companion baseline).

## R9 — Synthetic pip-app fixture (FR-009 proof)

**Decision**: Author a purely-in-Rust synthetic test at `waybill-cli/tests/cross_ecosystem_edges.rs` that constructs `Vec<PackageDbEntry>` directly (bypassing filesystem readers entirely) with:
- One `pkg:generic/my-pip-app@0.0.0-unknown` main-module with `depends: vec!["requests", "click"]`
- Two `pkg:pypi/` transitive components: `pkg:pypi/requests@2.31.0`, `pkg:pypi/click@8.1.7`

Then invoke the resolver pass (not the full scan) with FR-000 flag enabled and assert two `Relationship` edges emitted, each carrying the C137 annotation with `to_eco: "pypi"`.

**Rationale**: FR-009 requires ecosystem-agnosticism proof. Building a synthetic PackageDbEntry array in-process avoids the need for a real pip reader (which doesn't exist yet — it's a future milestone). Precedent: several existing waybill test files (e.g., `resolve/name_to_purl_test.rs`-adjacent tests) construct PackageDbEntry directly.

**Alternatives considered**:
- Copy the m216 gem builder pattern into a `pkg:generic/<slug>` mock via a real filesystem fixture with a fake `pyproject.toml`: rejected — introduces a fake pip reader; couples the test to fictional reader behavior.
- Wait for a real pip m216-alike reader before proving FR-009: rejected — the resolver fix has to be ecosystem-agnostic on merge, not "eventually".

## R10 — CI cost + flag-off byte-identity gate

**Decision**: Add a new integration test asserting SC-009 (flag-OFF byte-identity):

```rust
#[test]
fn flag_off_preserves_current_post_m216_byte_identity() {
    let out_default = run_scan(gemfile_fixture(), scan_args()); // no flag
    let out_control = read_committed_golden("m216_post_release_gemfile_cdx.json");
    assert_eq!(out_default, out_control, "flag-off must be byte-identical to m216 post-release golden");
}
```

**Rationale**: The gate is mechanical — one `assert_eq!` on two JSON strings. Golden committed at `waybill-cli/tests/fixtures/cross_ecosystem/golden_flag_off.cdx.json`. Regeneration only allowed via `MIKEBOM_UPDATE_CROSS_ECOSYSTEM_GOLDEN=1` env var and MUST be reviewed carefully at PR-time.
