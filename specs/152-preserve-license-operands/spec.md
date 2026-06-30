# Feature Specification: Preserve known operands in compound RPM license expressions (issue #481 fix)

**Feature Branch**: `152-preserve-license-operands`
**Created**: 2026-06-30
**Status**: Draft
**Input**: User description: "481 and yes let's do option 1"

## Origin & context

GitHub issue [#481](https://github.com/kusari-oss/mikebom/issues/481), filed 2026-06-29 by the maintainer, is a follow-up to issue #475 (closed by milestone #478 via commit `eb75853`). Milestone #478 added the `normalize_bitbake_license_operators` helper at `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:603` to translate Yocto BitBake-native `&` → `AND` and `|` → `OR` so the `spdx` crate's parser accepts the expression. That fix recovered 5 of the 10 NOASSERTION cases the maintainer originally surfaced on the `core-image-minimal` qemux86-64 scarthgap-LTS testbed.

The remaining 5 cases (`busybox`, `busybox-hwclock`, `busybox-syslog`, `busybox-udhcpc`, `liblzma5`) still emit `NOASSERTION` because the RPM `License:` headers contain compound expressions with at least one operand that isn't a registered SPDX identifier:

- `busybox*`: raw RPM header is something like `GPLv2 & bzip2-1.0.4`. After milestone-478 normalization → `GPLv2 AND bzip2-1.0.4`. `try_canonical` rejects this because `bzip2-1.0.4` isn't an SPDX-list id (closest is `bzip2-1.0.6`). The entire expression collapses to NOASSERTION, losing the recoverable `GPL-2.0-only` half.
- `liblzma5`: even more lossy — the expression is just `PD` (public domain). `PD` isn't a registered SPDX id, so the whole thing becomes NOASSERTION, losing the operator's clear intent.

The proposed fix (per #481, **option 1** — confirmed by maintainer):

> **Preserve known operands**: emit `GPL-2.0-only AND LicenseRef-bzip2-1.0.4`. The `LicenseRef-<unknown>` is an SPDX-spec-valid escape hatch for tokens that don't have a registered id. Downstream tooling can decide whether to ignore the LicenseRef or attempt to resolve it.

This milestone closes the residual NOASSERTION gap by:

1. Replacing the all-or-nothing `try_canonical` fallback with a two-pass strategy: first attempt full canonicalization (preserves existing happy-path behavior — no behavior change for fully-recognized expressions), then on failure, wrap each unrecognized operand as a `LicenseRef-<sanitized>` identifier and retry canonicalization.
2. Documenting the sanitization rule for unrecognized tokens so consumers can re-resolve LicenseRef tokens back to their original raw form when needed.

Per Constitution Principle V (standards-native first), the `LicenseRef-<idstring>` escape hatch is the spec-blessed SPDX 2.3 carrier for unknown identifiers — no new `mikebom:*` annotation introduced for this milestone. This keeps the fix tight + composes cleanly with every SPDX-aware downstream tool.

## Clarifications

### Session 2026-06-30

- Q: What sanitization rule transforms an unrecognized operand into the `<sanitized>` portion of `LicenseRef-<sanitized>`? → A: **Replace + collapse + strip** — replace each char outside `[a-zA-Z0-9-.]` with `-`, collapse consecutive `-` to single `-`, strip leading/trailing `-`. Idempotent. The information loss is acceptable because the LicenseRef escape hatch is documented as "opaque label; consult source if reversibility needed."
- Q: When the EXCEPTION of a `WITH` clause is unrecognized, what does mikebom emit for the surrounding compound expression? → A: **Whole compound → NOASSERTION**. An unknown exception modifies a license's legal meaning in load-bearing ways (e.g., GCC runtime library exception relaxes GPL terms); silently dropping or partially-canonicalizing the exception risks misleading auditors. The conservative whole-compound NOASSERTION fallback says "consult source" rather than asserting a license expression with unverified semantics.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Compliance auditor sees recoverable license info (Priority: P1)

A compliance auditor reading a mikebom-emitted SPDX 2.3 SBOM of a Yocto-built `core-image-minimal` image needs to see **as much license information as mikebom can recover** for every component, not have entire expressions silently collapsed to NOASSERTION because one operand is non-standard. Today, when an RPM declares `GPLv2 & bzip2-1.0.4`, the auditor sees `NOASSERTION` and has to manually inspect the source RPM header to recover the GPL-2.0-only portion. After this milestone, the auditor sees `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` and can immediately reason about the known half AND decide what policy to apply to the unknown half.

**Why this priority**: This is the direct fix for the user-filed issue. The information loss is real and measurable (5 of 35 installed packages on the testbed). Compliance audits are mikebom's documented persona-2 workflow (per the milestone-150 consumer guide §3.2); silently losing license signal undermines that workflow.

**Independent Test**: Scan the issue-#481 testbed (`yocto-test` local repo, `core-image-minimal` qemux86-64, scarthgap LTS, poky `802e4c1`) with the milestone-152 build and assert that the 5 affected packages' `licenseDeclared` fields now contain a non-NOASSERTION SPDX expression — the known operands preserved + the unknown operands wrapped as `LicenseRef-<sanitized>`.

**Acceptance Scenarios**:

1. **Given** an RPM with `License: GPLv2 & bzip2-1.0.4`, **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST be `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` (not `NOASSERTION`).
2. **Given** an RPM with `License: PD`, **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST be `LicenseRef-PD` (not `NOASSERTION`).
3. **Given** an RPM with `License: GPLv2 | bzip2-1.0.4`, **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST be `GPL-2.0-only OR LicenseRef-bzip2-1.0.4` (OR-operator path preserved).
4. **Given** an RPM with `License: GPLv2 & LGPLv2.1+`, **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST remain `GPL-2.0-only AND LGPL-2.1-or-later` (existing happy-path preserved — every operand is recognized, so the LicenseRef path doesn't fire).
5. **Given** an RPM with `License: ` (empty after trimming), **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST remain `NOASSERTION` (empty input is still NOASSERTION; LicenseRef path doesn't manufacture identifiers from nothing).
6. **Given** an RPM with `License: NotAnExpression!@#$` (raw garbage that doesn't decompose into operator/operand structure), **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST remain `NOASSERTION` (LicenseRef escape hatch fires per operand, not on opaque whole-string failures).

---

### User Story 2 — Idempotency + happy-path safeguards (Priority: P2)

A developer rebuilding mikebom on the same RPM testbed expects **byte-identical SBOM output** before vs. after the milestone-152 fix for every component whose existing expression was already fully canonicalizable. The new LicenseRef path MUST be a strict superset of the existing behavior — it activates only when the existing path returns NOASSERTION, and never alters a successful canonicalization.

**Why this priority**: prevents the fix from causing spurious changes to existing SBOM consumers' tooling pipelines. mikebom's downstream tools (sbomqs, syft/grype/trivy interop, sbomit, the milestone-072 cross-tier binding verifier) all consume mikebom SBOMs and could be sensitive to byte-level changes in license expressions.

**Independent Test**: Scan a fixture with only fully-recognized SPDX licenses (e.g., the existing milestone-090 sibling-fixture `transitive_parity/cargo` workspace) before + after the milestone-152 build; the emitted `licenseDeclared` fields MUST be byte-identical.

**Acceptance Scenarios**:

1. **Given** a fully-recognized SPDX expression (`MIT`, `Apache-2.0 AND MIT`, `GPL-2.0-only WITH GCC-exception-2.0`), **When** mikebom processes it, **Then** the emitted `licenseDeclared` MUST be the SAME canonical form as pre-milestone-152 mikebom would have produced.
2. **Given** a previously-failing expression (e.g., `GPLv2 & bzip2-1.0.4`), **When** processed twice (once raw, once feeding the milestone-152 output back as input), **Then** the second pass MUST produce the same output as the first (idempotency — feeding `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` back in yields itself, not double-wrapped).
3. **Given** an expression where the existing `normalize_bitbake_license_operators` helper still matches as the first transformation (i.e., raw input contains ` & ` or ` | `), **When** processed by milestone 152, **Then** the BitBake operator normalization MUST run first AS-IS, and the LicenseRef fallback only fires if the SPDX-operator-normalized expression still fails `try_canonical`.

---

### Edge Cases

- **Sanitization of unrecognized tokens**: SPDX 2.3 spec restricts the LicenseRef idstring grammar to `[a-zA-Z0-9-.]+`. Unrecognized operands containing characters outside that set (e.g., `GPLv2+`, `My License v2`, `(custom)`, `LGPL-2.1+`) MUST be sanitized so the resulting `LicenseRef-<sanitized>` is itself spec-valid. The sanitization rule MUST be documented (likely: replace each disallowed character with `-`, collapse consecutive `-` to single, strip leading/trailing `-`) so consumers can reverse-engineer the original token when needed.

- **Already-prefixed `LicenseRef-` tokens**: if a raw RPM header (improbable but possible) already contains `LicenseRef-something`, the milestone-152 path MUST NOT double-wrap it as `LicenseRef-LicenseRef-something`. Detection: any token already starting with `LicenseRef-` is treated as already-prefixed and passed through unchanged.

- **DocumentRef-prefixed tokens**: SPDX 2.3 also supports `DocumentRef-<docid>:LicenseRef-<idstring>` for cross-document references. Yocto's own SPDX 2.2 output emits these for `bzip2-1.0.4` (e.g., `DocumentRef-recipe-busybox:LicenseRef-bzip2-1.0.4`). The milestone-152 fix MUST NOT emit DocumentRef forms (mikebom has no document-reference context at the RPM-reader level). It emits the plain `LicenseRef-<sanitized>` form only. The Yocto-side DocumentRef form is richer but out of scope — adding it would require milestone-152 to know about Yocto's per-recipe SPDX document hierarchy, which is a separate concern.

- **WITH-clause operands**: SPDX `WITH` introduces an exception identifier (e.g., `GPL-2.0-only WITH GCC-exception-2.0`). The exception ID has its own registered list. Per Clarifications Q2: if the LEFT side of WITH is unrecognized → wrap as `LicenseRef-<sanitized>`. If the EXCEPTION (right side) is unrecognized → emit `NOASSERTION` for the entire surrounding compound expression (conservative; an unknown exception modifies legal meaning in load-bearing ways, e.g., GCC runtime library exception relaxes GPL terms — silently dropping or partially-canonicalizing risks misleading auditors).

- **Mixed AND/OR precedence**: SPDX `AND` binds tighter than `OR`. An expression like `MIT OR PD AND GPL-2.0-only` should parse as `MIT OR (PD AND GPL-2.0-only)`. After wrapping `PD` as `LicenseRef-PD`, the recombined expression `MIT OR LicenseRef-PD AND GPL-2.0-only` MUST preserve the original operator precedence (i.e., emit as either `MIT OR LicenseRef-PD AND GPL-2.0-only` matching SPDX precedence, OR explicitly parenthesized as `MIT OR (LicenseRef-PD AND GPL-2.0-only)`). The precedence-preservation rule MUST be implementation-decided during planning.

- **Parenthesized sub-expressions**: raw input `(GPLv2 OR LGPLv2.1+) AND PD` should preserve the parens through the LicenseRef-wrapping pass: emit `(GPL-2.0-only OR LGPL-2.1-or-later) AND LicenseRef-PD`.

- **Empty / whitespace-only input**: must remain NOASSERTION; the LicenseRef path doesn't manufacture an identifier from nothing.

- **Non-RPM ecosystems (deb, apk, etc.)**: deferred. Issue #481 is RPM-scoped (Yocto's testbed). If deb or apk readers exhibit the same NOASSERTION-on-unknown-operand collapse, that's a follow-up milestone (152b or 153). Per FR-009 below, this milestone touches the RPM reader only.

## Requirements *(mandatory)*

### Functional Requirements

#### Core fix (US1)

- **FR-001**: When an RPM's `License:` header contains a compound SPDX expression where at least one operand is unrecognized (not on the SPDX license list AND not already a `LicenseRef-`/`DocumentRef-` token), mikebom MUST preserve the recoverable portion by wrapping each unrecognized operand as `LicenseRef-<sanitized>` and emitting the recombined expression as the component's `licenseDeclared` field.

- **FR-002**: The sanitization rule for `LicenseRef-<sanitized>` MUST produce an idstring matching the SPDX 2.3 grammar `[a-zA-Z0-9-.]+` via the **replace + collapse + strip** algorithm (per Clarifications Q1): replace each character outside `[a-zA-Z0-9-.]` with `-`, collapse consecutive `-` into a single `-`, then strip leading and trailing `-`. The algorithm MUST be idempotent (running it on already-sanitized input yields the same input). The exact rule MUST be documented in the helper function's doc comment AND in the CHANGELOG.md entry for this milestone, so consumers can heuristically reverse-engineer the original raw token (full reversibility is not guaranteed; the SPDX `LicenseRef-` carrier itself doesn't promise it). Worked examples MUST appear in the helper's doc comment: `GPLv2+` → `GPLv2`, `My License v2` → `My-License-v2`, `(custom)` → `custom`, `LGPL-2.1+` → `LGPL-2.1`, `bzip2-1.0.4` → `bzip2-1.0.4` (unchanged — already valid).

- **FR-003**: When EVERY operand of a compound expression resolves cleanly via the existing `SpdxExpression::try_canonical` path, mikebom's emitted `licenseDeclared` MUST be byte-identical to pre-milestone-152 mikebom output for that component. The LicenseRef path is a fallback that activates ONLY on `try_canonical` failure.

- **FR-004**: Single-operand expressions with only an unrecognized token (e.g., `PD`, `bzip2-1.0.4`) MUST emit as `LicenseRef-<sanitized>` (not NOASSERTION). The LicenseRef escape hatch is per-operand, not per-compound-expression.

- **FR-005**: Operator preservation: AND, OR, and WITH operators MUST be preserved across the LicenseRef-wrapping pass. The recombined expression MUST be a valid SPDX 2.3 expression that `try_canonical` accepts on a second pass. Operator precedence per SPDX spec (`AND` binds tighter than `OR`) MUST be preserved; parenthesized sub-expressions in the raw input MUST round-trip through the LicenseRef-wrapping pass unchanged.

#### Idempotency + happy-path safeguards (US2)

- **FR-006**: Feeding milestone-152 LicenseRef-wrapped output back in as input MUST be idempotent — `LicenseRef-bzip2-1.0.4` stays as `LicenseRef-bzip2-1.0.4` (not `LicenseRef-LicenseRef-bzip2-1.0.4`). Tokens already starting with `LicenseRef-` MUST be detected and passed through unchanged.

- **FR-007**: The milestone-478 `normalize_bitbake_license_operators` helper (BitBake `&`/`|` → SPDX `AND`/`OR` normalization) MUST run FIRST in the pipeline, AS-IS, with no behavioral change. The milestone-152 LicenseRef fallback activates ONLY when the SPDX-operator-normalized expression still fails `try_canonical`. The pipeline order is: raw → normalize_bitbake_operators → try_canonical → (on failure) → LicenseRef-wrap → try_canonical → (on failure) → NOASSERTION.

- **FR-008**: Empty / whitespace-only input MUST remain NOASSERTION. Truly opaque garbage that doesn't decompose into operator/operand structure (e.g., a raw token containing only invalid characters that sanitization would strip to empty) MUST also remain NOASSERTION — the LicenseRef path doesn't manufacture identifiers from nothing.

#### Scope guards (out-of-scope statements as requirements)

- **FR-009**: This milestone modifies the RPM reader (`mikebom-cli/src/scan_fs/package_db/rpm_file.rs`) ONLY. Other readers (deb_file, apk_file, gem, npm lockfile, etc.) are NOT touched. If they exhibit the same NOASSERTION-on-unknown-operand collapse, that's a follow-up milestone — DELIBERATELY out of scope for #481 closure.

- **FR-010**: This milestone does NOT introduce any new `mikebom:*` annotation key (per Constitution Principle V — the SPDX-native `LicenseRef-<idstring>` carrier is the standards-native solution; no parity-bridging annotation needed).

- **FR-011**: This milestone does NOT emit `DocumentRef-<docid>:LicenseRef-<idstring>` forms (the cross-document form is Yocto-specific context the RPM reader doesn't have). Only the plain `LicenseRef-<sanitized>` form is emitted.

- **FR-012**: This milestone does NOT change the `licenseConcluded` field — only `licenseDeclared`. `licenseConcluded` is operator-asserted (per the `--conclude-licenses` flag + the milestone-132 `mikebom:license-concluded-source` annotation) and is out of scope.

#### WITH-clause behavior (edge-case guard)

- **FR-013**: When the LEFT side of a `WITH` clause is unrecognized, mikebom MUST wrap it as `LicenseRef-<sanitized>` and re-attempt canonicalization. When the EXCEPTION (RIGHT side) of a `WITH` clause is unrecognized, mikebom MUST emit **NOASSERTION for the entire surrounding compound expression** (per Clarifications Q2). The LicenseRef escape hatch is for LICENSE identifiers, not EXCEPTION identifiers (SPDX 2.3 does not define an `ExceptionRef-` form), and an unknown exception modifies a license's legal meaning in load-bearing ways (e.g., GCC's runtime library exception relaxes GPL terms); silently dropping or partially-canonicalizing the exception would risk misleading auditors. Whole-compound NOASSERTION says "consult source" rather than asserting a license expression with unverified semantics.

### Key Entities

- **Compound license expression**: a raw RPM `License:` header value that consists of one or more operands joined by SPDX operators (`AND`, `OR`, `WITH`) with optional parens.
- **Operand**: a single license identifier within a compound expression (e.g., `GPLv2`, `bzip2-1.0.4`, `PD`).
- **Recognized operand**: an operand that, on its own, passes `SpdxExpression::try_canonical` (i.e., is a registered SPDX license list id OR an already-prefixed LicenseRef-/DocumentRef-LicenseRef token).
- **Unrecognized operand**: an operand that fails `try_canonical` on its own (not on the SPDX list, not already a LicenseRef-).
- **Sanitized LicenseRef token**: `LicenseRef-<sanitized>` where `<sanitized>` is the unrecognized operand transformed to match the SPDX 2.3 idstring grammar (`[a-zA-Z0-9-.]+`).
- **The 5 affected packages** (the issue #481 reference set, used as the SC-001 acceptance fixture): `busybox`, `busybox-hwclock`, `busybox-syslog`, `busybox-udhcpc`, `liblzma5`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (the 5-package fix)**: After milestone 152 ships, scanning the issue-#481 testbed (`yocto-test` local repo, `core-image-minimal` qemux86-64, scarthgap LTS, poky `802e4c1`) MUST result in non-NOASSERTION `licenseDeclared` for all 5 affected packages (`busybox`, `busybox-hwclock`, `busybox-syslog`, `busybox-udhcpc`, `liblzma5`). Specifically: 4 busybox-* packages emit `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` (or equivalent canonical form per the sanitization rule); `liblzma5` emits `LicenseRef-PD` (or equivalent).

- **SC-002 (zero regressions on happy path)**: After milestone 152 ships, scanning the existing milestone-090 sibling-fixture cargo + npm + go testbeds and comparing the emitted SPDX 2.3 `licenseDeclared` fields against pre-milestone-152 output MUST show byte-identical results for every package whose existing expression was fully canonicalizable. (Verified mechanically via the existing milestone-090 golden test infrastructure + a milestone-152-specific diff check.)

- **SC-003 (idempotency)**: Feeding milestone-152 LicenseRef-wrapped output (e.g., the literal string `"GPL-2.0-only AND LicenseRef-bzip2-1.0.4"`) back as input to the license-processing function MUST produce the same output unchanged — no double-wrapping, no operator drift.

- **SC-004 (broader Yocto coverage)**: After milestone 152 ships, the percentage of packages with non-NOASSERTION `licenseDeclared` on the issue-#481 testbed MUST be ≥30/35 (≥86%). Pre-#475 baseline was 25/35 (71%); post-#478 baseline is 30/35 (86%); post-#152 target is 35/35 (100%) for THIS testbed — any residual NOASSERTION cases MUST be documented as either (a) genuinely-empty `License:` headers or (b) WITH-clause-exception-unrecognized cases per FR-013.

- **SC-005 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-152 main (clippy clean + every test passes except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake).

- **SC-006 (new unit-test coverage)**: At least 8 new unit tests in `mikebom-cli/src/scan_fs/package_db/rpm_file.rs` covering: (a) the 5 affected real-world expressions from issue #481, (b) idempotency on already-LicenseRef-prefixed input, (c) happy-path no-op (fully-canonical input round-trips unchanged), (d) sanitization edge cases (chars outside `[a-zA-Z0-9-.]+`), (e) WITH-clause behavior per FR-013, (f) parens preservation. Each test MUST exercise the new code path independently (no fixture cross-contamination).

- **SC-007 (no new annotation keys / wire-format changes)**: The shipped diff MUST NOT touch `docs/reference/sbom-format-mapping.md` (no new catalog rows). The CycloneDX / SPDX 2.3 / SPDX 3 emitters' wire shapes MUST be unchanged — the only behavioral delta is that `licenseDeclared` (SPDX 2.3) / `licenses[].license.id` (CDX) / `software_packageLicenseDeclared` (SPDX 3) values are now richer for the 5 affected packages and equivalents.

- **SC-008 (documentation update)**: The shipped diff MUST include a brief note in CHANGELOG.md naming the LicenseRef escape-hatch behavior + the sanitization rule, so consumers reading the alpha.NN → alpha.NN+1 changelog can adapt their license-policy filters if they pattern-match on `NOASSERTION` (which they should no longer see for the 5 affected packages).

## Assumptions

1. **`spdx` crate API surface** (version pinned in the workspace `Cargo.toml`): `SpdxExpression::try_canonical(&str) -> Result<SpdxExpression, _>` continues to exist and is the canonical entry point for expression validation. The crate's tree-walking + parse APIs are sufficient for the LicenseRef-wrapping pass — no new crate added.

2. **Sanitization rule documentation**: the exact transformation (e.g., "replace disallowed chars with `-`, collapse consecutive `-` to single, strip leading/trailing `-`") is implementation-decided during planning, but MUST be documented in the helper's doc comment + the CHANGELOG.md entry. Consumers reverse-engineering the original raw token (e.g., to look up a license on a third-party tracker) should be able to apply the inverse transformation.

3. **Testbed access**: the maintainer (mike@kusari.dev) has the issue-#481 testbed locally at `yocto-test/` and can re-run the SC-001 verification post-merge. There is no automated CI test against the Yocto testbed (it's not in the milestone-090 sibling-fixture repo); the SC-001 verification is a manual operator-cadence check, similar to milestones 478 + 150.

4. **No RPM fixture in sibling repo**: the milestone-090 sibling-fixture repo does NOT carry RPM fixtures with compound `&`/`|` license headers. Therefore the new unit tests (SC-006) use synthetic strings hardcoded inline in `rpm_file.rs`'s test module, matching the existing milestone-478 test pattern at `rpm_file.rs:1061+`.

5. **`LicenseRef-` is consumer-recognized**: SPDX-aware downstream tools (syft, grype, trivy, sbomqs, FOSSology) recognize `LicenseRef-<idstring>` as a legitimate SPDX 2.3 license-expression element and DO NOT crash on them. This is a spec-blessed escape hatch, not a mikebom invention.

6. **Happy-path byte-identity is the testable SC-002 contract**: pre-/post-152 byte-comparison of the SPDX 2.3 output for fully-recognized expressions IS the regression guard. The milestone-090 golden test infrastructure provides the pre-152 snapshot; SC-002 is verified by a fresh scan vs the golden.

7. **Pipeline ordering matters**: BitBake-operator-normalization MUST run BEFORE LicenseRef-wrapping, because raw `GPLv2 & bzip2-1.0.4` has TWO problems (the `&` operator AND the `bzip2-1.0.4` operand). The milestone-478 normalizer fixes the operator; the milestone-152 fallback fixes the operand. Running them in the wrong order would either re-introduce the operator problem or fail to detect the operand problem.

8. **The `licenseConcluded` field is out of scope**: per FR-012, only `licenseDeclared` is touched. `licenseConcluded` is operator-asserted and follows a separate code path (the milestone-132 `--conclude-licenses` + `mikebom:license-concluded-source` infrastructure).

9. **Cross-format consistency**: although the issue is filed against SPDX 2.3 output, the underlying license-data structure is `Vec<SpdxExpression>` on `PackageDbEntry` and is consumed by all three format emitters (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1). The fix propagates uniformly through the existing per-format builders without per-format special-casing. No format emitter is modified.

## Dependencies

- **Milestone #478** (closed 2026-06-29): added the `normalize_bitbake_license_operators` helper that this milestone composes with as the first pipeline step. The helper stays unchanged; milestone 152 adds a SECOND fallback after it.
- **`mikebom-common::types::license::SpdxExpression`**: the newtype already in workspace use; provides `try_canonical` and tree-walking APIs.
- **`spdx` crate** (existing workspace dep, version pinned at the time of milestone 152): provides expression parsing and the LicenseRef grammar reference.

## Out of Scope

- No deb_file / apk_file / gem / npm / etc. license-processing changes. Other readers may exhibit the same NOASSERTION collapse but addressing them is a follow-up milestone per FR-009.
- No new `mikebom:*` annotation keys per FR-010.
- No `DocumentRef-` form emission per FR-011 (would require Yocto-specific per-recipe document-hierarchy context that the RPM reader doesn't have).
- No `licenseConcluded` changes per FR-012 — only `licenseDeclared`.
- No exception-identifier escape hatch per FR-013 — SPDX 2.3 doesn't define `ExceptionRef-`.
- No `--licenseref-disable` opt-out flag. The behavior is a strict improvement (replaces NOASSERTION with structured-but-unknown info); no operator could rationally want the old NOASSERTION behavior back. If a hypothetical consumer breaks, the fix is on the consumer side (handle `LicenseRef-` per SPDX 2.3 spec).
- No CHANGELOG.md tooling automation. The SC-008 update is a hand-authored line.
- No retroactive re-scoring of milestone-090 golden fixtures (they don't carry Yocto RPMs).
