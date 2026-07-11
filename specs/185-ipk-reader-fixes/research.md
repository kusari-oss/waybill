# Research: ipk reader bug fixes (m185)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decisions

### Decision 1 — Filename parser semantic: `rsplitn(3, '_')` from the right

**Decision**: Replace the current `parse_ipk_filename` implementation (`stem.split('_')` with strict `parts.len() != 3` guard) with `rsplitn(3, '_')` that extracts arch → version → name from the right. The remainder-after-second-rightmost-underscore becomes the name; the middle field is the version (may itself contain underscores); the rightmost field is the arch.

**Rationale**:
- ipk-spec's filename shape (`<name>_<version>_<arch>.ipk`) uses `_` as the OUTER field separator, but the version field itself is opaque and may legally contain `_` (e.g., BitBake's `SRCPV` expansion producing `6.6.127+git0+45f69741c7_70af2998be-r0`).
- Right-to-left splitting correctly handles: (a) the canonical 2-underscore case (rsplitn(3) produces exactly 3 parts — same as split() for 2-underscore input), (b) multi-underscore version case (rsplitn(3) still produces exactly 3 parts, joining any additional underscores into the version field), (c) malformed short input (rsplitn(3) produces fewer than 3 parts → None-return preserved).
- `rsplitn(N, delim)` is stdlib; zero new dependencies.

**Alternatives considered**:
- **Regex-based parse** — rejected. Adds `regex` dependency footprint (already a workspace dep, but adds parsing complexity for a case a stdlib call handles cleanly).
- **Full ipk-spec grammar tokenizer** — over-engineered for the two-field split. ipk-spec doesn't define a formal grammar for the filename; the `<name>_<version>_<arch>` convention is a wire-format expectation, not a grammar rule.
- **Length-limited split allowing 4 parts** — semantically identical to `rsplitn(3)` but requires post-processing to rejoin the first N-2 parts into the name. `rsplitn(3)` does this in one call by returning an iterator that stops after the third element from the right.

**Empty-field guard**: preserved. If any of the 3 extracted fields is empty (post-`trim`), the parser returns None. Matches pre-m185 behavior for the malformed case.

---

### Decision 2 — Reuse rpm reader's helpers via `pub(crate)` visibility bump

**Decision**: Promote `rpm_file.rs::normalize_bitbake_license_operators` (line 615) and `rpm_file.rs::preserve_known_operands_with_license_ref` (line 832) from private `fn` to `pub(crate) fn`. `opkg.rs::build_entry` calls them via `super::rpm_file::<name>` for the first two normalization passes. Zero behavior change on the rpm side.

**Rationale**:
- The rpm reader has already solved the exact class of problem opkg needs to solve (per-#475/#481 hardening chain). Reusing the proven helpers avoids duplicating logic + duplicating test coverage.
- Visibility bump (`fn` → `pub(crate) fn`) is a semantic no-op — no callers outside the module change (opkg is inside the same crate, in the `package_db` sibling module). The rpm call site continues to reference the helpers via unqualified name; no rpm behavior changes.
- Deferred alternative: extracting the helpers into a `mikebom-cli/src/scan_fs/package_db/license/` shared module would be cleaner long-term but is out-of-scope for m185's bug-fix delivery. The `pub(crate)` bump is a MINIMAL-TOUCH step that unlocks reuse without introducing new module structure.

**Alternatives considered**:
- **Duplicate helper implementations in opkg.rs** — rejected. Divergent code paths for the same normalization would violate DRY + double the maintenance burden for future SPDX-hardening PRs.
- **Move helpers to a new shared module `license.rs`** — rejected for m185 scope. Introduces module-structure churn; better addressed as its own refactoring milestone if third or fourth readers need the pipeline (e.g., apk, deb).
- **Move helpers to `mikebom-common`** — over-scoped. The rpm-side helpers are inherently reader-family utilities, not shared-common data-model utilities.

**FR-011 preservation**: rpm's call site at `rpm_file.rs:479-488` uses the same 3-pass pipeline as pre-m185. Zero behavior change on rpm.

---

### Decision 3 — Opkg-side 4th-pass wholesale-wrap fallback

**Decision**: When BOTH the first-pass (`SpdxExpression::try_canonical` on the operator-normalized raw string) AND the second-pass (`preserve_known_operands_with_license_ref` + re-canonicalize) fail on an opkg License string, the m185 opkg reader wraps the WHOLE original string as a single `LicenseRef-<sanitized>` operand and emits it. Per the m185 Q1 clarification.

**Rationale**:
- Preserves the raw string for downstream license auditors (they can grep the sanitized form to recover the original text) instead of dropping to `licenses: []` / NOASSERTION.
- Aligns with the FR-014 spec text — see the m185 Clarifications section.
- Does NOT apply to the rpm reader — rpm keeps its current 3-pass behavior. If rpm ever needs the same wholesale-wrap semantic, a separate follow-up milestone can extend rpm's call site to invoke the same fallback (or move both call sites onto a shared 4-pass pipeline).
- Sanitization rule: reuse the existing `sanitize_to_license_ref_idstring` at `rpm_file.rs:770` (also promoted to `pub(crate)` per Decision 2's scope). The rule replaces non-`[A-Za-z0-9.-]` characters with `-`, aligning with SPDX 2.3 §10's `idstring` grammar.
- If the wholesale-wrap ITSELF produces a value that fails `SpdxExpression::try_canonical` (extremely unlikely given the sanitization rule strips all non-`[A-Za-z0-9.-]` characters), the emitted component falls back to `licenses: []` (matches FR-007 absent-License regression pin).

**Alternatives considered**:
- **Apply the 4th-pass to rpm too** — rejected. Would drift rpm goldens (any rpm License string that currently drops to NOASSERTION on both passes would become a LicenseRef- in m185). SC-005 prohibits rpm-golden drift for non-Yocto ecosystems. Extending to rpm requires a separate milestone with its own regen coordination.
- **Emit tracing::warn! only, no LicenseRef- fallback** — rejected per the user's Q1 selection (Option A). Would preserve pre-m185 opkg absence.
- **Emit tracing::warn! AND LicenseRef- fallback** — considered. Adds observability at zero implementation cost. Adopted as a supplementary detail (not a decision-level alternative — see Data Model §3.2's `tracing::warn!` recommendation).

**Handling the "wholesale-wrap of malformed input" edge**: if the sanitization strips ALL characters (e.g., an all-whitespace or all-punctuation License string), the resulting `LicenseRef-` prefix has no idstring suffix — invalid per SPDX 2.3 §10. In that case, the wholesale-wrap fallback ALSO returns None; the emitted component correctly falls to `licenses: []` per FR-007. This makes the 4th-pass fully defensive.

---

### Decision 4 — `sanitize_to_license_ref_idstring` visibility (co-promoted with Decision 2's helpers)

**Decision**: `rpm_file.rs::sanitize_to_license_ref_idstring` (line 770) also gets promoted from `fn` to `pub(crate) fn`. Used by the m185 wholesale-wrap fallback (Decision 3) to compute the LicenseRef- suffix.

**Rationale**:
- Same reuse logic as Decision 2 — the sanitization rule is a proven implementation of SPDX 2.3 §10's `idstring` grammar; duplicating it in opkg.rs would violate DRY.
- Visibility bump only — zero behavior change on rpm.

**Follow-up**: total m185 visibility bumps: 3 functions in `rpm_file.rs` (2 primary helpers per Decision 2 + 1 sanitizer per Decision 4). No other rpm_file.rs changes.

---

### Decision 5 — Fixture scope: synthesize m185 test cases; do NOT rely on external Yocto fixture

**Decision**: m185 unit tests use inline synthetic inputs (fake ipk filenames + inline opkg-status stanza strings). No new external fixture directory is added to `mikebom-test-fixtures`. SC-002 / SC-004's "stock Yocto image" language is treated as an aspirational validation target — verified via unit tests that mimic the observed shapes (4 kernel-module filename shape for SC-002; 5-package opkg-status stanza for SC-004).

**Rationale**:
- Matches the m183/m184 precedent (both deferred external-fixture-directory work as separate follow-ups; unit-test coverage sufficed for gating).
- Adding a real Yocto rootfs to `mikebom-test-fixtures` requires (a) a hosted `core-image-minimal` image (~200 MB), (b) legal review of the embedded GPL/etc. content, (c) cross-repo PR coordination. Out-of-scope for a surgical bug-fix delivery.
- The observed real-world shapes are documented in the m185 spec.md's US1 acceptance scenario 4 and US2 acceptance scenario 1 — synthetic tests can precisely replicate them.

**Alternatives considered**:
- **Vendored micro-fixture (~1 MB)**: rejected. Even a stripped-down `core-image-minimal` would exceed 10 MB in size, and the specific 4-kernel-module + 5-related-package pattern requires the FULL image to reproduce. Synthetic tests are more focused.
- **Cross-repo PR to `mikebom-test-fixtures`**: deferred per m183/m184 pattern. Documented in tasks.md as `[~]` deferred.

---

## Bug Discovery

**Observation**: While researching Decision 2, the actual rpm normalization pipeline (`rpm_file.rs:469-488`) already produces `licenses: Vec::new()` when BOTH first-pass and second-pass fail. The Q1 clarification's text ("matches the rpm reader's per-#481 fail-safe wholesale-wrap behavior") is technically imprecise — rpm's #481 wraps INDIVIDUAL OPERANDS (per-token), not the whole string.

**Impact on m185 delivery**: none. The user's Q1 selection (Option A) was to ADD wholesale-wrap for opkg, regardless of whether rpm implements it. m185 US2 follows the user's chosen semantic without extending it to rpm (per Decision 3's FR-011 preservation).

**Impact on future milestones**: if a follow-up milestone extends the wholesale-wrap to rpm, that milestone's spec should explicitly document the rpm-golden regen and remove the FR-011-equivalent invariant for that scope.
