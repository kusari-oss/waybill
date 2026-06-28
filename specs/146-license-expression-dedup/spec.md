# Feature Specification: SPDX license expression operand dedup

**Feature Branch**: `146-license-expression-dedup`
**Created**: 2026-06-28
**Status**: Draft
**Input**: User description: "Dedupe identical operands in SPDX license expressions before emission — fixes issue #470 where Yocto-built RPMs ship `License: GPL-2.0-only AND GPL-2.0-only` in headers, and mikebom passes the duplication through verbatim into emitted SBOMs (declaredLicense, licenseConcluded, CDX licenses[], SPDX 3 software_declaredLicense)."

## Origin

External Yocto-test testbed feature `002-recipe-level-rollup` (2026-06-26 audit) filed against `kusari-oss/mikebom` as [issue #470](https://github.com/kusari-oss/mikebom/issues/470). Comparing mikebom's SPDX 2.3 output against Yocto's native SPDX for `core-image-minimal` on qemux86-64, every shared component (35/35 in the installed-on-image subset) had a self-duplicated `licenseDeclared`:

| Yocto SPDX `licenseDeclared` | mikebom equivalent |
|---|---|
| `GPL-2.0-only` | `GPL-2.0-only AND GPL-2.0-only` |
| `Apache-2.0` | `Apache-2.0 AND Apache-2.0` |
| `MIT` | `MIT AND MIT` |
| `Zlib` | `Zlib AND Zlib` |
| `GPL-2.0-or-later` | `GPL-2.0-or-later AND GPL-2.0-or-later` |
| `LGPL-2.1-or-later` | `LGPL-2.1-or-later AND LGPL-2.1-or-later` |

7 distinct duplicated expressions observed across ≥30 of the 35 shared components.

**Root cause** (verified during /speckit-specify investigation):

1. Yocto's RPM build process literally writes the joined-form license string into the RPM `License:` header (e.g., `License: GPL-2.0-only AND GPL-2.0-only`) when multiple recipe variables contribute the same identifier (a quirk of Yocto's bitbake LICENSE handling).
2. mikebom's `mikebom_common::types::license::SpdxExpression::try_canonical` calls `spdx::Expression::parse + to_string()` on the raw header value. Verified via standalone test: the `spdx = "0.10"` crate's parse + canonical-display round-trip preserves duplicate operands verbatim — `MIT AND MIT` parses successfully and renders back as `MIT AND MIT` (no idempotent simplification).
3. Downstream emitters (`spdx/packages.rs::reduce_license_vec` at line 237-256, `cyclonedx/builder.rs`, `spdx/v3_licenses.rs`) consume the SpdxExpression as-is and serialize the duplicated form into `licenseDeclared` (SPDX 2.3) / `software_declaredLicense` (SPDX 3) / `licenses[].license.id|expression` (CDX 1.6).

Per SPDX 2.x grammar, `X AND X` is logically equivalent to `X` (AND is idempotent). The duplicated form is harmless but noisy, surprises downstream consumers comparing license strings, and inflates the cross-format diff surface for tools like sbomqs, syft-comparators, and license-compliance gates.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Identical top-level AND-operands are collapsed (Priority: P1)

A compliance engineer scans a Yocto-built image and emits SPDX 2.3 + SPDX 3 + CDX. They expect each component's declared/concluded license to be the canonical form — `GPL-2.0-only`, not `GPL-2.0-only AND GPL-2.0-only`. Today every component whose upstream license string contains a self-AND duplicate (Yocto RPMs, some pip wheels, some npm packages with concatenated SPDX-License-Identifier headers) emits the noisy form, breaking byte-equality comparisons across tools and inflating downstream license-compliance review effort. After this milestone, top-level AND-operands that are byte-identical are collapsed to a single occurrence.

**Why this priority**: This is the dominant audit signal (≥30 of 35 shared components on the Yocto baseline; 7 distinct duplicated expressions). The fix is small (under 100 LOC), localized to `mikebom_common::types::license`, and benefits every downstream emitter at once. Compliance tooling (license-policy gates, SBOM diffing, sbomqs license-quality checks) is the principal beneficiary.

**Independent Test**: Construct a `SpdxExpression` from a `MIT AND MIT` raw string, call the new dedup method, assert the result's string form is `MIT`. Then run an integration scan against any RPM whose `License:` header carries the duplicated form and assert the emitted CDX `licenses[]`, SPDX 2.3 `licenseDeclared`, and SPDX 3 `software_declaredLicense` all carry the deduplicated single-id form.

**Acceptance Scenarios**:

1. **Given** a `SpdxExpression` constructed from `"MIT AND MIT"`, **When** the dedup pass runs, **Then** the canonical string form is `MIT` (single id, no AND operator).
2. **Given** a `SpdxExpression` constructed from `"MIT AND Apache-2.0 AND MIT"`, **When** the dedup pass runs, **Then** the canonical string form is `MIT AND Apache-2.0` (preserves the first occurrence; second `MIT` collapsed).
3. **Given** a `SpdxExpression` constructed from `"GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later AND GPL-2.0-only"`, **When** the dedup pass runs, **Then** the canonical string form is `GPL-2.0-only AND LGPL-2.1-or-later` (3 GPL-2.0-only occurrences collapse to 1; LGPL preserved in its original position relative to the surviving GPL).
4. **Given** a `SpdxExpression` constructed from `"MIT AND Apache-2.0"` (no duplication), **When** the dedup pass runs, **Then** the canonical string form is unchanged (`MIT AND Apache-2.0`).
5. **Given** an emitted SBOM (any of CDX 1.6, SPDX 2.3, SPDX 3) from a scan whose underlying upstream license string was `X AND X`, **When** the operator inspects the `licenseDeclared` / `software_declaredLicense` / CDX `licenses[].license.id` field, **Then** the value is the single id `X`, not the duplicated form `X AND X`.

---

### User Story 2 - Identical top-level OR-operands are collapsed (Priority: P2)

The OR operator is also idempotent in SPDX 2.x grammar (`X OR X ≡ X`). Less commonly observed in practice than the AND case but covered by the same dedup logic. After this milestone, `MIT OR MIT` collapses to `MIT`; `MIT OR Apache-2.0 OR MIT` collapses to `MIT OR Apache-2.0`.

**Why this priority**: Same fix shape as US1 (one extra line in the dedup pass to handle OR alongside AND). Lower observed frequency in the Yocto audit (Yocto's RPM concatenation produces AND, not OR), but covering OR closes the symmetric gap and avoids a follow-up milestone if a future audit surfaces the OR case.

**Independent Test**: Same shape as US1 with OR substituted for AND.

**Acceptance Scenarios**:

1. **Given** a `SpdxExpression` constructed from `"MIT OR MIT"`, **When** the dedup pass runs, **Then** the canonical string form is `MIT`.
2. **Given** a `SpdxExpression` constructed from `"MIT OR Apache-2.0 OR MIT"`, **When** the dedup pass runs, **Then** the canonical string form is `MIT OR Apache-2.0`.

---

### Edge Cases

- **`WITH` clauses preserved untouched**: `GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later WITH Classpath-exception-2.0` — both top-level operands are byte-identical full WITH-clauses. The dedup MUST treat each full `<license> WITH <exception>` as a single atomic operand and collapse them if byte-identical, NOT split across the WITH boundary and dedupe just the license-id half. So `GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later` does NOT dedupe (the two operands differ — one has the WITH exception, the other doesn't); whereas `GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later WITH Classpath-exception-2.0` collapses to a single occurrence.
- **Different SPDX-id case** (e.g., `mit AND MIT`): SPDX 2.x identifiers are case-sensitive per the spec, but the `spdx` crate's canonical form normalizes to the registered casing. After `try_canonical`, both surface as `MIT`. The dedup compares post-canonical-form strings, so case-only differences are collapsed (the upstream canonicalization handles it).
- **Whitespace-only differences** (`MIT AND  MIT`): the `spdx` crate's canonical form normalizes whitespace, so both operands surface as `MIT` post-canonical and dedupe correctly.
- **Parenthesized sub-expressions** (`(MIT AND MIT) OR Apache-2.0` and `MIT OR (MIT AND Apache-2.0)`): the dedup operates on TOP-LEVEL operands only (relative to the parser's tree). Parenthesized sub-expressions are NOT recursively deduped in v1.0. The first form `(MIT AND MIT) OR Apache-2.0` is treated as two top-level OR operands `(MIT AND MIT)` and `Apache-2.0` — neither is byte-identical to the other, so no dedup fires. (Out of Scope §1 records this deferral.)
- **`X AND Y` where Y is a substring of X but not identical** (`GPL-2.0-only AND GPL-2.0`): different strings post-canonical, no dedup.
- **Empty expression / parse-failure**: the dedup pass is a no-op for `SpdxExpression` values that don't parse as a SPDX 2.x grammar tree (the existing `SpdxExpression::new` constructor, used when `try_canonical` fails, stores the raw string verbatim — no dedup applies).
- **Single-operand expressions** (`MIT` alone): dedup is a no-op.
- **Already-canonical mikebom-emitted goldens**: most existing golden fixtures have single-id licenses (`MIT`, `Apache-2.0`) — no dedup change expected. Any golden that happens to contain a `X AND X` pre-fix shape MUST be refreshed in the same PR.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `mikebom_common::types::license::SpdxExpression` MUST gain a deterministic dedup pass that collapses byte-identical top-level operands in the parsed expression tree. The pass MUST handle the `AND` operator (US1) and the `OR` operator (US2) symmetrically.
- **FR-002**: The dedup pass MUST preserve the original order of first occurrence — `MIT AND Apache-2.0 AND MIT` → `MIT AND Apache-2.0` (the second `MIT` is the duplicate; the first survives).
- **FR-003**: The dedup pass MUST treat `<license-id> WITH <exception-id>` as a single atomic operand. Splitting across the `WITH` boundary is forbidden (would change SPDX semantics — `GPL WITH Classpath` is a different license from bare `GPL`).
- **FR-004**: The dedup pass MUST be a no-op for single-operand expressions (`MIT` alone), already-deduplicated expressions (`MIT AND Apache-2.0`), and expressions with parenthesized sub-expressions whose top-level operands aren't byte-identical (parenthesized sub-expressions are NOT recursively deduped in v1.0 — see Out of Scope §1).
- **FR-005**: The dedup pass MUST be applied AUTOMATICALLY during `SpdxExpression::try_canonical` so that all downstream consumers (CDX builder, SPDX 2.3 emitter, SPDX 3 emitter, license-comparison tests, parity catalog rows) see the deduplicated form without per-consumer wiring.
- **FR-006**: The existing `SpdxExpression::new` constructor (which stores raw strings verbatim when `try_canonical` fails) MUST NOT apply the dedup pass — preserving its existing "best-effort raw storage" contract for invalid-but-not-empty inputs.
- **FR-007**: All existing byte-identity SBOM golden tests MUST be inspected for pre-fix `X AND X` patterns; any golden containing such a pattern MUST be refreshed in the same PR. The refresh diff MUST be limited to license-string simplifications (single grep-able pattern: `"<X> AND <X>"` → `"<X>"`); no unrelated golden drift.
- **FR-008**: The fix MUST be observable in CDX 1.6, SPDX 2.3, and SPDX 3 output formats simultaneously — all three downstream emitters consume the same `SpdxExpression`, so a single fix at the type level satisfies cross-format invariance.
- **FR-009**: All changes MUST preserve Constitution Principle V (standards-native > `mikebom:*`) — no new `mikebom:*` annotations introduced; the milestone fixes how an EXISTING type (`SpdxExpression`) is normalized.

### Key Entities

- **`SpdxExpression`** — `mikebom_common::types::license::SpdxExpression`, a newtype around `String` representing a SPDX 2.x license expression. Pre-146: `try_canonical` returns the `spdx` crate's parse-and-display round-trip verbatim, which preserves duplicate operands. Post-146: `try_canonical` additionally applies a top-level operand-dedup pass before storing the canonical string.
- **License expression operand** — A leaf node in the SPDX 2.x expression grammar tree. Either a bare SPDX identifier (`MIT`), an identifier with an exception (`GPL-2.0-or-later WITH Classpath-exception-2.0`), or a parenthesized sub-expression (`(MIT AND Apache-2.0)`). The dedup pass operates on top-level operands of a single AND/OR list — it does NOT recurse into parenthesized sub-expressions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After this milestone, scanning the Yocto baseline corpus (the audit's `core-image-minimal` qemux86-64 build) produces zero `licenseDeclared` / `licenseConcluded` values containing the substring `X AND X` (where X is any single SPDX identifier). Verifiable via `jq -r '.packages[] | select(.licenseDeclared | test("^([^ ]+) AND \\1$")) | .name' <emitted-spdx-2.3>` returning empty.
- **SC-002**: A unit test in `mikebom-common::types::license::tests` asserts that `SpdxExpression::try_canonical("MIT AND MIT").unwrap().as_str() == "MIT"`. Plus 4-6 sibling tests for the other acceptance scenarios from US1 + US2 (multi-operand mixed, OR, WITH preservation, no-op cases).
- **SC-003**: Byte-identity golden tests for any fixture containing a pre-fix `X AND X` license value are refreshed; the diffs show ONLY license-string simplifications (single grep-able pattern). Most fixtures are unaffected (their license strings are single ids); only the rpm-fixture-derived goldens may shift.
- **SC-004**: An integration test constructs a synthetic RPM with `License: MIT AND MIT` in its header, scans it, and asserts the emitted CDX + SPDX 2.3 + SPDX 3 outputs all carry `MIT` (single id), not `MIT AND MIT`.
- **SC-005**: A regression-guard test asserts that `SpdxExpression::try_canonical("GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later")` does NOT collapse (the two operands differ — one has the WITH exception, the other doesn't). This guards FR-003.
- **SC-006**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` both pass clean — i.e., `./scripts/pre-pr.sh` exits 0 (excepting the pre-existing local sbomqs_parity env-only failure documented in milestone-144 T001).
- **SC-007**: The sbom-conformance harness, when re-run by the operator post-merge, reports zero `licenseDeclared X AND X` findings (was 7 distinct expressions × ≥30 components on the audit baseline).

## Assumptions

- **The `spdx` crate's `Expression::parse` is the canonical source for the parsed tree structure.** We use the crate's existing parser to enumerate top-level operands (the crate exposes the parsed expression via methods on `Expression`); we don't reimplement SPDX 2.x grammar parsing. If the crate's API for tree enumeration is insufficient, a fallback string-split-on-` AND `/` OR ` approach works for the dominant case (no nested parens / WITH at the top level), with the limitation that complex nested forms aren't deduped — but those are out of scope for v1.0 per Out of Scope §1.
- **AND and OR idempotency are universally safe semantic transformations** per the SPDX 2.x grammar. No consumer can break by gaining a deduped expression that was previously duplicated; the SPDX spec defines `X AND X ≡ X` and `X OR X ≡ X` as equivalences.
- **The fix at `try_canonical` is the right architectural choice** (vs. at the emitter sites). Reasons: (a) one code change covers CDX + SPDX 2.3 + SPDX 3 + future formats; (b) any test or other consumer comparing license strings benefits transparently; (c) preserves the round-trip invariant for consumers that compare `SpdxExpression` values for equality.
- **No semantic loss from dedup.** The duplication carries no information — both operands are byte-identical canonical SPDX ids. `MIT AND MIT` and `MIT` are equivalent SPDX expressions per the spec; the duplication is noise.
- **No new Cargo dependencies needed.** The `spdx` crate is already in the workspace; no new crates required. The dedup pass uses standard Rust string + collection operations.
- **The Yocto-side root cause (bitbake LICENSE concatenation producing duplicated header values) is out of scope.** That's a Yocto / OE-core issue to address upstream if anyone cares; mikebom's responsibility is to normalize the canonical form on output regardless of input shape.
- **Operator semantics**: AND has lower precedence than WITH but higher than OR per SPDX 2.x. The dedup operates on whichever top-level connector dominates the expression tree (AND or OR, whichever is the outermost). Mixed AND/OR expressions like `MIT OR Apache-2.0 AND MIT` are parsed by the `spdx` crate per SPDX precedence (`MIT OR (Apache-2.0 AND MIT)` per standard precedence) — the top-level is OR with two operands; neither is byte-identical to the other, so no dedup fires. The inner AND-expression `Apache-2.0 AND MIT` is not recursively examined (out of v1.0 scope).

## Out of Scope

- **Recursive dedup into parenthesized sub-expressions.** `(MIT AND MIT) OR Apache-2.0` would NOT have its inner `MIT AND MIT` deduped in v1.0. The top-level OR operands `(MIT AND MIT)` and `Apache-2.0` are not byte-identical, so no top-level dedup fires; the inner AND isn't recursively examined. Practical impact: zero observed cases in the Yocto audit (the duplication is always at the top level when upstream license-tag concatenation is the source).
- **Algebraic simplification beyond operand dedup.** `(MIT AND Apache-2.0) OR (Apache-2.0 AND MIT)` is logically equivalent to `MIT AND Apache-2.0` but the simplification requires expression-tree normalization (commutativity, associativity, distribution) — out of scope.
- **Cross-tier license merging** (e.g., merging two ResolvedComponent's licenses during dedup at `resolve/deduplicator.rs`) — that's a separate concern; the deduplicator already doesn't merge licenses (confirmed during /speckit-specify investigation, see audit trail in the issue thread). If a future milestone wants to merge cross-component licenses, this milestone's operand-dedup pass would apply automatically to the merged expression.
- **Changing the upstream RPM `License:` value at the reader.** The mikebom rpm reader reads the header verbatim; normalization happens during `SpdxExpression::try_canonical`. Touching the reader-side parsing is unnecessary.
- **A new `SpdxExpression::dedupe()` method called manually by emitters.** The fix lives inside `try_canonical` to be transparent. If a future caller wants to opt out (preserve raw form), they can use `SpdxExpression::new` (which already skips canonicalization).
- **License-policy gates** that act on the deduplicated form (e.g., "reject any SBOM with `MIT AND MIT`" — a useful CI lint elsewhere). Out of scope; the mikebom-side fix means such gates wouldn't fire anyway post-146 on mikebom-produced SBOMs.
- **CDX 1.6 license-expression dedup at the `license.expression` shape** specifically (CDX uses `license.id` for single identifiers and `license.expression` for compound). The CDX emitter at `cyclonedx/builder.rs` already routes to `id` for single-identifier `SpdxExpression`s via the existing `as_single_identifier()` method (verified at `mikebom-common/src/types/license.rs:73`). Post-146, a previously-compound `MIT AND MIT` becomes single-identifier `MIT` and naturally routes to the `id` shape — improving CDX schema-validation rates as a side benefit. No CDX-emitter code change required.
- **New `mikebom:*` annotations.** No new properties introduced; the fix transforms an EXISTING value's canonical form.
