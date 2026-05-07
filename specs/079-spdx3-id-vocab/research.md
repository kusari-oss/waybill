# Research — milestone 079 SPDX 3 externalIdentifierType conformance

Five implementation-level decisions to pin before Phase 1 design. The two highest-impact decisions (Q1: comment-field slot; Q2: gitoid-only detection) were locked during /speckit.clarify; this document documents the remaining details and validates them against ground truth (the local SPDX 3 schema + the existing milestone-074 `git:` value-shape contract).

## §1 — Per-scheme mapping table (definitive)

**Decision**: The complete mikebom-scheme → SPDX 3 controlled-vocabulary mapping is:

| mikebom scheme | Source | SPDX 3 `externalIdentifierType` | `comment` value | Notes |
|---|---|---|---|---|
| `image` | milestone 074 image-tier auto-detect, or `--component-id <PURL>=image:...` | `other` | `"original-scheme: image"` | Value can be a registry URL (e.g. `registry.example.com/img:tag`) or a digest reference; either way, `other`. `urlScheme` is forbidden because SPDX 3 reserves it for IANA URI schemes (`mailto:`, `tel:`), not arbitrary URLs. |
| `repo` | milestone 074 source-tier auto-detect (git remote URL extraction), or `--component-id <PURL>=repo:...` | `other` | `"original-scheme: repo"` | Always `other`. |
| `git` (value matches `^[0-9a-f]{40}$`) | milestone 074 source-tier auto-detect always emits this exact shape (see §2) | `gitoid` | None — no info loss because the vocab value `gitoid` already carries the "git object ID" semantic. | Per Q2. The auto-detect path always lands here. |
| `git` (value does NOT match the regex) | Reachable only via `--component-id <PURL>=git:<not-a-SHA>` (user-supplied non-SHA git value, e.g., a `git+https://` URL) | `other` | `"original-scheme: git"` | Per Q2 fallback. |
| `subject` | milestone 076 build-tier subject identifiers from in-toto witness-v0.1 envelopes | `other` | `"original-scheme: subject"` | Subjects are name+digest tuples that don't map to any single vocab value cleanly. |
| `attestation` | milestone 076 build-tier attestation references | `other` | `"original-scheme: attestation"` | |
| User-defined `--component-id <PURL>=<SCHEME>:<VALUE>` where `<SCHEME>` is in the SPDX 3 vocab (`cve`, `cpe23`, `swhid`, `swid`, `gitoid`, `cpe22`, `urlScheme`, `email`, `securityOther`, `packageUrl`, `other`) | milestone 073 `--component-id` flag | The vocab value verbatim (e.g., `<SCHEME>=cve` → `"cve"`) | None — operator chose a vocab-conformant name. | The mapping function detects this case and short-circuits. |
| User-defined `--component-id <PURL>=<SCHEME>:<VALUE>` where `<SCHEME>` is NOT in the vocab (e.g., `jira`, `internal-ticket`) | milestone 073 | `other` | `"original-scheme: <SCHEME>"` (e.g., `"original-scheme: jira"`) | Per FR-003. The five built-in scheme names (`repo`/`git`/`image`/`attestation`/`subject`) are rejected at flag parse time per `component_id.rs:52`, so user-defined `<SCHEME>` cannot collide with the auto-detect paths. |

**Schema validation**: confirmed against `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json`:
- The `Core/ExternalIdentifierType` SHACL constraint enumerates exactly: `cpe22, cpe23, cve, email, gitoid, other, packageUrl, securityOther, swhid, swid, urlScheme` (plus a `BlankNodeOrIRI` extension reference path mikebom doesn't use).
- The `Core/ExternalIdentifier` element's `comment` property is defined at `prop_ExternalIdentifier_comment` with `type: "string"` — directly on the element, not inherited from a parent.
- Required fields on `Core/ExternalIdentifier` are `externalIdentifierType` + `identifier`. `comment` is optional. Adding it is purely additive.

**Rationale**: Every input shape has a defined output. The mapping is a total function. The `gitoid` case (auto-detect path) preserves more semantic precision than uniform `other` mapping while staying validator-clean.

**Alternatives considered**:
- `image` → `urlScheme` when value is URL-shaped — Rejected per Q2 + spec Edge Case: SPDX 3 reserves `urlScheme` for IANA URI schemes, not URLs.
- `attestation` → `swhid` (Software Heritage Persistent Identifier) — Rejected: `swhid` has a strict `swh:1:` syntax; mikebom attestation references aren't SWHIDs.
- Map `--component-id <PURL>=cve:CVE-1234` to `other` for safety — Rejected: per FR-001 the emission MUST use a vocab value when one is named; passing through `cve` is the operator's explicit choice.

## §2 — `git:` value-shape catalog + gitoid regex precision

**Decision**: The detection regex stays exactly as Q2 specified: `^[0-9a-f]{40}$`. No SHA-256 expansion needed.

**Validation**: mikebom's milestone-074 `git:` value emission path is bounded by `git_rev_parse_head` at `mikebom-cli/src/binding/identifiers/auto_detect.rs:578-603`. That function explicitly rejects (returns `None`) any output that isn't exactly 40 lowercase hex characters per VR-074-003. Comment at line 589-592: "must be exactly 40 lowercase hex chars; anything else (abbreviated SHA, ref name leaking through, empty output) returns `None` to preserve the wire-format invariant from milestone 073's `validate_git`." So:

- mikebom's auto-detect `git:` values: ALWAYS exactly 40-char hex SHA-1. Always match the regex. Always map to `gitoid`.
- User-supplied `--component-id <PURL>=git:<anything>` values: NOT POSSIBLE — `git` is a reserved built-in scheme name and the `--component-id` parser rejects it per `component_id.rs:52`. So `git:` values reach mikebom only via the auto-detect path described above. The fallback regex case in the mapping function (FR-004) exists defensively for code-correctness but is not reachable from operator input today.

SHA-256 git SHAs (64-char hex) are out of scope: `validate_git` doesn't accept them, and mikebom's auto-detect doesn't probe for SHA-256 git repos. Since `git:` is a reserved built-in scheme that user-defined `--component-id` cannot use, this case is unreachable from operator input today. If a future milestone extends the auto-detect path to produce SHA-256 git SHAs, the regex would naturally extend — no API change required.

Abbreviated SHAs (7-12 char hex prefixes): not produced by `git_rev_parse_head` (the function explicitly enforces 40-char SHA-1 per `auto_detect.rs:589-602`). Same reachability constraint as SHA-256: not reachable from operator input.

**Rationale**: The regex precisely matches mikebom's authoritative auto-detect output, with predictable fallback for user-supplied values. No false positives possible (a 40-char hex string that ISN'T a git SHA would be exotic — and even then, emitting it as `gitoid` is still spec-conformant because `gitoid` accepts any git object ID string format).

**Alternatives considered**:
- Validate the value against actual git repository state — Rejected: O(I/O) at emission time + breaks offline-emission contract from milestone 075.
- Detect SHA-256 git SHAs (64-char hex) speculatively — Rejected: mikebom doesn't produce them today, and a speculative regex on user input might over-match (a long hex digest that isn't a git SHA).
- Drop gitoid detection entirely (always emit `other` for `git:`) — Rejected: would lose semantic precision for the dominant auto-detect case where we KNOW it's a git SHA.

## §3 — `Core/ExternalIdentifier.comment` field shape verification

**Decision**: Use the `comment` field at the `Core/ExternalIdentifier` element level (NOT the higher-class `Core/Element.comment` that other element types inherit). The schema confirms `comment` is defined directly on `Core/ExternalIdentifier` at `prop_ExternalIdentifier_comment` (per the schema audit during /speckit.plan Phase 0). Field type is `string`. No SHACL content constraint beyond the type.

**Comment string format**: `format!("original-scheme: {scheme_name}")` — exact format. Examples:
- `"original-scheme: image"`
- `"original-scheme: subject"`
- `"original-scheme: jira"` (user-defined)

**Why a structured prefix**: downstream tooling (cross-tier correlation, SBOM-diff tools, mikebom's own `verify-binding` / `trace-binding` from milestone 072) can parse the `original-scheme: ` prefix to recover the original mikebom scheme deterministically. A bare scheme name in `comment` would be ambiguous (operators sometimes write free-text comments unrelated to the original scheme); the prefix disambiguates.

**Schema-conformance**: a `string` value matches the schema constraint. SHACL imposes no further restriction. Validator passes — confirmed at the data-model level.

**Rationale**: Avoids reaching for `Core/Element.comment` (which exists higher in the inheritance chain and might or might not apply via the JSON-LD framing rules; safer to use the directly-attached property). Free-text format with structured prefix balances human-readability against machine-parseability.

**Alternatives considered**:
- `identifierLocator` field — Rejected per Q1: semantic mismatch (it's "where to find this identifier" — meant for retrieval URLs/IRIs, not metadata about the original scheme name). Also it's an array per the schema, so it'd be more awkward to use.
- `issuingAuthority` field — Rejected: stronger semantic mismatch (the original mikebom scheme is not an "issuing authority" for the identifier).
- Drop the structured prefix and put just the scheme name in `comment` — Rejected: ambiguity with free-text operator comments. The prefix costs ~16 bytes per identifier and removes ambiguity.

## §4 — Determinism contract for the new `comment` field + sort key

**Decision**: The mapping function is a pure function of `(scheme: SchemeName, value: &str)` returning `(SpdxIdType, Option<String>)`. Same inputs → byte-identical output. Per FR-005.

**Sort-key extension for the externalIdentifier array**: today `mikebom-cli/src/generate/spdx/v3_external_ids.rs` sorts the per-component identifier list by `(externalIdentifierType, identifier)` for determinism. Post-fix, two distinct identifiers with the same `(mapped vocab value, identifier value)` but different original schemes (e.g., the same component carries `--component-id <PURL>=jira:X` AND `--component-id <PURL>=ticket:X` — both map to `(other, X)` with different `original-scheme:` comments) would have the same primary sort key but different `comment` fields. To preserve sort-key uniqueness + dedup correctness, extend the sort key to `(externalIdentifierType, identifier, comment.unwrap_or(""))`. Identifiers with identical `(type, identifier, comment)` triples ARE genuinely duplicates (same source intent + same target value + same vocab mapping) and SHOULD dedup to one entry — matching today's contract. Note: the auto-detect built-ins can't collide with user-defined identifiers because the `--component-id` parser rejects the five reserved scheme names.

**No determinism risk from the regex**: the gitoid regex is compiled once with `lazy_static!` or `OnceLock` (per existing project patterns) and evaluates deterministically per (scheme, value) input. No clock, no PRNG, no thread-local state.

**Multi-identifier edge case** (per spec Edge Cases section): a single component carrying both an auto-detected `subject:foo` (build-tier from milestone 076) and a user-defined `--component-id <PURL>=jira:PROJ-1234` produces two `externalIdentifier` array entries — first sorted by mapped vocab value (`other` for both), then by identifier value (`PROJ-1234` < `foo`), then by comment. Stable + deterministic + correct.

**Rationale**: The smallest sort-key extension that preserves both determinism AND uniqueness. No behavioral change for SBOMs that don't exercise the new mapping.

**Alternatives considered**:
- Sort by tuple `(externalIdentifierType, identifier, original_scheme)` instead of `(..., comment)` — Rejected: requires re-deriving the original scheme from the comment to sort, which is computationally redundant. Sort by the actual emitted-string value.
- Don't extend the sort key (rely on dedup uniqueness from `(type, identifier)` alone) — Rejected: would cause the multi-source edge case to silently drop one identifier or produce non-deterministic output.

## §5 — CDX 1.6 + SPDX 2.3 negative-confirmation audit

**Decision**: Confirmed by code-path inspection that CDX 1.6 + SPDX 2.3 emission paths read mikebom's internal `SchemeName` types via INDEPENDENT code paths from the SPDX 3 emission code path. Per FR-006 + FR-010 + FR-011, those paths are not modified by this milestone.

**Audit trail**:
- CDX 1.6 emission: `mikebom-cli/src/generate/cyclonedx/` reads `Identifier { scheme, value }` and emits to `externalReferences[].type` — CDX 1.6's own controlled vocabulary (independent of SPDX 3's). The CDX 1.6 vocab includes `vcs`, `documentation`, `support`, `distribution`, `website`, etc. — wholly different from SPDX 3's. mikebom's CDX path uses its own scheme→type mapping that's not affected by this milestone.
- SPDX 2.3 emission: `mikebom-cli/src/generate/spdx/document.rs` reads the same `Identifier` types and emits to `externalRefs[].referenceCategory` + `externalRefs[].referenceType`. SPDX 2.3 has its own vocabulary (`SECURITY`/`PACKAGE-MANAGER`/`PERSISTENT-ID`/`OTHER` for category; `cpe23Type`/`purl`/`swh`/etc. for type). Independent.
- SPDX 3 emission: `mikebom-cli/src/generate/spdx/v3_document.rs:309` + `v3_packages.rs:170` are the ONLY two call sites that emit `externalIdentifierType`. This milestone modifies exactly these two call sites (via the new helper).

**Verification at gate-time**: post-fix, run `cargo test --test cdx_regression` and `cargo test --test spdx_regression` WITHOUT `MIKEBOM_UPDATE_*_GOLDENS` env vars; both must pass without regen, confirming byte-identity preservation across CDX 1.6 + SPDX 2.3.

**Rationale**: Code-path independence is the cleanest backward-compat guarantee. The existing test infrastructure already encodes the byte-identity contract; running it post-fix without regen is the gate.

**Alternatives considered**:
- Refactor to a shared scheme-to-type mapping across all three formats — Rejected: would force CDX 1.6 + SPDX 2.3 goldens to regenerate (since the unified mapping might emit slightly different per-format strings even if semantics are equivalent), violating FR-006. Keep the per-format mappings independent.
