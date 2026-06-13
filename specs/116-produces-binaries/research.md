# Research — Automatic binary-name binding via produces-binaries annotation

**Feature**: 116-produces-binaries
**Date**: 2026-06-13
**Status**: Decisions resolved; no NEEDS CLARIFICATION markers remaining.

## Decision 1 — Field name + Constitution Principle V audit

**Decision**: `mikebom:produces-binaries`. JSON-array-of-strings value. Stamped on source-tier main-module components via the existing `extra_annotations: BTreeMap<String, serde_json::Value>` channel (same path C40 `mikebom:component-role` takes; see `mikebom-cli/src/scan_fs/package_db/cargo.rs:363-368` for the canonical emission pattern). Documented as a new row in `docs/reference/sbom-format-mapping.md` per Principle V's documentation requirement.

**Constitution Principle V bullet 5 audit** (the bullet that requires "standards-native fields take precedence over `mikebom:`-prefixed properties; spec authors MUST audit each target format for an existing native construct… and cite the audit result"):

- **CycloneDX 1.6**: No native field carries "this package produces these executable names" semantics. The closest neighbor is `externalReferences[type=executable]`, but that field expresses a URL (homepage / distribution / VCS endpoint), not a list of output-binary identifiers — semantically wrong shape. `component.evidence.identity[field=name]` carries name-discovery confidence, not output-name capability. `component.evidence.callstack` is for runtime call traces, not declared outputs.
- **SPDX 2.3**: No native field. `Package.packageFileName` is the canonical filename of the SOURCE artifact (the SBOM-described thing), not the produced binary outputs. `Package.externalRefs[]` mirrors CDX (URL-focused). `Package.annotation[]` is the documented extensibility point — same path the existing C1–C45 `mikebom:*` annotations already follow.
- **SPDX 3.0.1**: Same shape as 2.3. `ExternalIdentifier[]` is for CPE/advisory references. `Annotation` is the documented extensibility mechanism.

**Conclusion**: No native field exists. `mikebom:produces-binaries` is justified per Principle V bullet 5 as a parity-bridging annotation. The audit result MUST be cited in `docs/reference/sbom-format-mapping.md` alongside the existing C1–C45 rows.

**Alternatives considered**:
- **`externalReferences[type=executable]` with the value being a comma-separated string of names** — rejected: semantically wrong (the field is for URL endpoints), would confuse consumers reading the SBOM by violating type expectations.
- **A new `evidence.callstack.frame.binary` shape** — rejected: `evidence.callstack` is for runtime evidence; mikebom doesn't observe runtime invocations at source-tier scan.
- **Splitting into per-binary `mikebom:produces-binary` properties (one property per name)** — rejected: violates the SBOM-property convention of one key per semantic concept; downstream consumers would have to scan all properties matching a prefix instead of reading one.

## Decision 2 — Field value shape

**Decision**: JSON array of strings, lex-sorted, deduped. All entries are lowercase ASCII with platform-specific suffixes (`.exe`, `.jar`) STRIPPED. The JSON is encoded as a `serde_json::Value::Array` in the `extra_annotations` map; CDX serialization renders it as a single `properties[]` entry whose `value` is the JSON-encoded string of the array; SPDX 2.3/3 renders it via the existing `MikebomAnnotationCommentV1` envelope wrapper (same pattern as the milestone-072 / milestone-111 binding envelopes).

**Rationale**: Sorted+deduped matches FR-012's union-merge contract (operator pre-seeded values + mikebom-discovered values → de-duplicated set → lex-sorted output). Lex-sorted means byte-deterministic output across runs; sorted+deduped means SBOM-quality consumers reading the property don't see superficial drift from ordering noise. Stripped suffixes match the source-tier canonical form (the binder's FR-002 suffix tolerance handles the image-side `.exe`/`.jar` translation; the source side never carries them — confirmed via spec clarification Q2). Lowercase matches the FR-002 case-insensitive match contract (the source-side normalization makes the matching cheap at bind-time — no per-call `to_lowercase()`).

**Alternatives considered**:
- **JSON object mapping `name → path-of-origin`** — rejected: leaks an implementation detail (the file the name came from); the property is a CAPABILITY claim ("this source produces these names"), not a DETECTION TRACE.
- **Comma-separated string** — rejected: harder for downstream consumers to parse; non-standard for JSON-shaped properties; doesn't carry the FR-013 collision audit cleanly.
- **Unsorted** — rejected: would produce SBOM-output drift across runs / hosts even when the underlying source is unchanged.

## Decision 3 — How the binder reads the declaration at bind-time

**Decision**: Extend `SourceSbomContext` at `mikebom-cli/src/binding/verify.rs:460-474` with a new field:

```rust
/// Maps lowercase extensionless binary names → source-tier PURL(s) declared
/// via `mikebom:produces-binaries` on the source SBOM's components. Multi-
/// valued because FR-013's name-collision case MUST be reported as `weak`
/// with `multiple-source-candidates-for-binary-name` reason; the binder
/// needs all candidates to populate the audit trail.
binary_name_to_purl: HashMap<String, Vec<Purl>>,
```

`SourceSbomContext::load()` at line 478 populates the index by scanning every component's `properties[]` (CDX path) / `annotation[]` (SPDX path) for the `mikebom:produces-binaries` key. Names go in as-is (already lowercase + extensionless per Decision 2). `binding_for_purl()` at line 520 gains a fallback branch: when exact PURL match fails AND the incoming PURL is `pkg:generic/<name>` AND the index contains `<name>` (after FR-002 normalization: `to_lowercase()` + strip trailing `.exe`/`.jar`), use the index entry to produce a binding result.

**Rationale**: The index makes the auto-alias lookup O(1) per binding query, vs. O(N-components) if we re-scanned the source SBOM each time. The Vec-valued shape handles FR-013's collision case (multiple source candidates for the same name) without breaking the existing single-candidate fast path. The binder reads the property via the same `extra_annotations` deserialization path the source SBOM already uses, so no new SBOM parser code is needed.

**Alternatives considered**:
- **Walk the source SBOM lazily on every `binding_for_purl()` call** — rejected: O(N²) at scale; the index is cheap (~10 ms one-time at scan startup per Technical Context's perf model).
- **Materialize as `HashMap<String, Purl>` and silently drop colliding entries** — rejected: violates FR-013 (collision MUST surface as `weak`).
- **Cache the index across scans on disk** — rejected: violates the spec's Storage assumption (N/A — in-process per scan); also adds cache-invalidation complexity for no observable win.

## Decision 4 — `SourceDocumentBinding` envelope extension

**Decision**: Add ONE new field to the `SourceDocumentBinding` struct at `mikebom-cli/src/binding/mod.rs:187-217`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub alias_source: Option<AliasSource>,
```

with a new enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AliasSource {
    OperatorSupplied,
    AutomaticFromProducesBinaries,
}
```

The `alias_source` field is populated WHENEVER `alias_from` / `alias_to` are populated (the paired-presence invariant extends to include `alias_source`). Old envelopes lacking the field deserialize cleanly via `#[serde(default)]` and present as `None`; for old SBOMs with operator-supplied aliases (milestone-111 era), consumers MAY treat absent `alias_source` as implicitly `OperatorSupplied` since automatic-alias didn't exist pre-feature.

**Rationale**: Mirrors milestone-111's pattern exactly — paired-presence + `skip_serializing_if = "Option::is_none"` + serde-default for backwards-compat deserialization. The kebab-case enum rendering matches the existing SBOM-property idiom ("operator-supplied" not "operatorSupplied"). The enum variants are explicit (no `Other` variant) — adding a third alias source in the future is a deliberate constitution-amendment-level decision, not a silent compat carve-out.

**Alternatives considered**:
- **Use `Option<String>` instead of an enum** — rejected: violates Constitution Principle IV (Type-Driven Correctness — domain values use newtype enums, not raw `String`).
- **Pair `alias_source` with `alias_from` / `alias_to` as a 3-tuple newtype** — rejected: would force a breaking rename of the existing milestone-111 fields; the additive `Option<AliasSource>` lets pre-feature SBOMs deserialize unchanged.

## Decision 5 — Cargo `src/bin/*.rs` enumeration

**Decision**: Use `scan_fs::walk::safe_walk` (milestone 114) with `max_depth: 1` (`src/bin/` is a single-level directory by Cargo convention; nested dirs are not implicit-binaries). The `should_skip` closure rejects anything that's NOT a regular file with a `.rs` extension. The binary name is the file stem (e.g., `src/bin/foo.rs` → `foo`).

**Rationale**: Reuses milestone-114's shared helper rather than hand-rolling a `read_dir` loop — keeps the new walker through the milestone-115 walker-audit allow-list automatically (the audit grep matches `fn walk[_(]`; since we go through `safe_walk` we don't add a new walker function). Cargo's implicit-binary rule is strictly `src/bin/<name>.rs` (one level deep); `src/bin/<subdir>/main.rs` is NOT an implicit binary per Cargo docs, so `max_depth: 1` is correct.

**Alternatives considered**:
- **Hand-rolled `read_dir`** — rejected: would add a new entry to the walker-audit allow-list for no benefit.
- **Walk to arbitrary depth and let Cargo's resolver figure it out** — rejected: Cargo's implicit-binary rule is depth-1 only; deeper directories are NOT implicit binaries and producing names for them would be wrong.

## Decision 6 — PR-split strategy

**Decision**: Three sequential PRs matching the US priorities:

- **PR-A**: Foundation (envelope extension + binder index + auto-alias resolution) + Cargo extractor + backwards-compat test. US1 MVP. After merge, the issue body's Rust workflow is closed end-to-end.
- **PR-B**: npm + pip + gem + maven extractors. US2. Binder unchanged from PR-A.
- **PR-C**: Go extractor with `package main` filesystem walk. US3.

**Rationale**: PR-A is the smallest end-to-end vertical — it ships the binder change WITH one ecosystem so reviewers can evaluate the cross-tier contract concretely (not abstractly). PR-B fans out to four more ecosystems with no binder change, making per-ecosystem review easy. PR-C is isolated because Go's filesystem-walk extraction is fundamentally different from the manifest-based extractors. The order matches the spec's US priority order (P1 → P2 → P3). Internally, PR-B may be sub-split into four smaller PRs if reviewer feedback on PR-B's diff size pushes back.

**Alternatives considered**:
- **One mega-PR covering all six ecosystems** — rejected: diff too large to review; the binder change is conceptually independent from the per-ecosystem extractors and reviewers should evaluate them separately.
- **PR-A = binder only, PR-B = all six ecosystems** — rejected: PR-A would have no integration test path (the binder needs at least ONE source-side extractor to test the end-to-end flow). Bundling Cargo with the binder gives PR-A an exercisable contract.
- **Five sequential PRs (binder, cargo, npm, pip, gem, maven, golang)** — rejected: over-decomposed. The npm/pip/gem/maven extractors are roughly equivalent in complexity and follow the same pattern; bundling them is fine for review.

## Decision 7 — Image-side suffix-tolerance scope (FR-002)

**Decision**: `.exe` (Windows) + `.jar` (JVM) ONLY. No `.dll`, `.so`, `.dylib`, `.bin`, `.com`, `.bat`, `.ps1` tolerance.

**Rationale**:
- `.exe` covers Windows-built images (binaries on Linux typically have no suffix; binaries on Windows always do).
- `.jar` covers maven shaded-JAR outputs (the source-side declares the extensionless `baz`; the image-side discovery often produces `baz.jar`).
- `.dll`/`.so`/`.dylib` are LIBRARIES, not executables; they should not bind to a source-side "produces this binary" declaration. If an operator's image contains both `baz` (binary) and `libbaz.so` (library), only `baz` should bind. Tolerating `.so` would risk false positives.
- `.bin`/`.com`/`.bat`/`.ps1` are exotic enough to defer; if a user pushes back on a specific case post-merge, adding them is a one-line change.

**Alternatives considered**:
- **All-common-extensions tolerance** — rejected: invites false positives at the `weak`-strength tier, eroding the contract's signal.
- **Configurable suffix list via env var** — rejected: per spec clarification Q2 design ("the binder owns all platform-suffix translation"), the suffix-list is part of mikebom's contract, not operator-configurable. Adding an env var would invite operators to silently misconfigure.

## Decision 8 — Env-var parity with milestone-111's `MIKEBOM_PKG_ALIAS`

**Decision**: NO env-var override for the automatic-alias path. The feature is opt-in by virtue of using `--bind-to-source` (the existing milestone-072 flag); operators who don't use `--bind-to-source` never see the new behavior. There is no `MIKEBOM_PRODUCES_BINARIES_AUTO_ALIAS=0` flag.

**Rationale**: Per spec Assumptions, "Operators who want to OPT OUT of the automatic alias path can suppress it by simply not using `--bind-to-source`, which is the only flow this feature affects. There is no per-component or per-scan opt-out; the absence of such opt-out is intentional — adding one would invite operators to silently suppress real bindings as workaround for unrelated problems." Milestone 111's `MIKEBOM_PKG_ALIAS` env var is a different shape (a value-carrier for operator-supplied data, parallel to `--pkg-alias`), not an opt-out flag. There's no operator-supplied value to carry for the automatic path; the source-side declaration IS the value.

**Alternatives considered**:
- **`MIKEBOM_PRODUCES_BINARIES_AUTO_ALIAS=0` opt-out env var** — rejected per the spec's "no bypass mechanism" stance (mirrors milestone 115's walker-audit gate decision).
- **`MIKEBOM_PRODUCES_BINARIES=...` value-carrier (operator can pre-stuff binary names)** — rejected: the operator-supplied path is ALREADY served by milestone-111's `--pkg-alias` flag, which is more precise (explicit LHS-RHS pair) and doesn't muddy the source-tier declaration's "this is what the source TOOLING says" semantics.
