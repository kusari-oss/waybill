# Follow-up constitution amendment: Principle V "CISA 2025" → "CISA 2026"

**Status**: Draft — do NOT open this PR from the feature 221 branch.
File in a separate branch after `221-cisa-2026-elements-audit` merges,
so the constitution edit + the compliance work land as two reviewable
diffs.

**Type**: MINOR bump — 2.0.0 → 2.1.0. The 2026 elements are a strict
superset of 2025; no principle is removed, redefined, or made
incompatible with prior interpretation.

---

## Suggested PR title

```
docs: constitution v2.1.0 — bump Principle V compliance target to CISA 2026 Minimum Elements
```

## Suggested PR body

```markdown
## Summary

- Principle V (Specification Compliance) currently references
  "CISA 2025 Minimum Elements" as the required compliance baseline.
- CISA published the "2026 Minimum Elements for a Software Bill of
  Materials" on 2026-07-29 (TLP:CLEAR); the 2026 revision is a strict
  superset of 2025 (adds 9 elements, renames 3, retires SWID from
  Machine-Processable Data).
- Milestone 221 (feature `221-cisa-2026-elements-audit`) landed the
  full compliance work — the machine-verified matrix at
  `docs/cisa-2026-coverage.md`, `--sign-key` for SBOM Author
  Signature, doc-scope SBOM Generation Context in SPDX 2.3 + SPDX 3,
  and `--sbom-version <N>` for SBOM Version.
- This PR updates the constitution to reflect the new target baseline
  so future feature specs cite CISA 2026 by default.

## Diff

The only material change is Principle V line 211:

```diff
- - **CISA 2025 Minimum Elements** — all required fields
+ - **CISA 2026 Minimum Elements** — all required fields
    populated, including "Tool Name" as `waybill` and
-   "Generation Context" reflecting active build-time trace.
+   "Generation Context" reflecting active build-time trace
+   (waybill's `GenerationContext` variants map to CISA's
+   `before-build` / `build` / `after-build` vocabulary via
+   `waybill_common::attestation::metadata::GenerationContext::as_cisa_2026_lifecycle`;
+   see `docs/cisa-2026-coverage.md` for the full coverage matrix).
```

Also update the front-matter SYNC IMPACT REPORT + `Version` + `Last
Amended` fields; regenerate the report body with:

- Version change: 2.0.0 → 2.1.0
- Bump rationale: MINOR — Principle V's normative content is
  materially expanded to reflect the CISA 2026 Minimum Elements
  supersession of the 2021 NTIA + 2025 CISA baselines. The 2026
  publication adds 9 data-field elements (SBOM Author Signature,
  Data Format Name/Version, Generation Context, Tool Name/Version,
  SBOM Version, Component Hash Value/Algorithm, Component License),
  renames Supplier Name → Component Producer, Depth → Coverage,
  Known Unknowns → Explicitly Identifying Unknown Information, and
  drops SWID from Machine-Processable Data. Every principle's
  NORMATIVE CONTENT is unchanged apart from this reference update.
- Modified sections: Principle V bullet 1 (compliance target),
  version field, last-amended field.
- Added sections: cross-reference to `docs/cisa-2026-coverage.md`
  in Principle V.
- Removed sections: none.
- Templates requiring updates: none — the spec-kit templates cite
  Principle V by name, not by the specific standards version, and
  the change is additive.

## Test plan

- [ ] `docs/cisa-2026-coverage.md` cited from Principle V renders as
      a valid relative link.
- [ ] Machine-verifying integration test
      `waybill-cli/tests/cisa_2026_coverage_matrix.rs` still passes
      (already gated on every CI build; this PR touches only markdown).
- [ ] `cargo +stable clippy --workspace --all-targets -- -D warnings`
      (constitution changes shouldn't affect code, but constitution's
      Pre-PR Verification section is authoritative).
- [ ] `cargo +stable test --workspace` per the same section.

## Motivation

Waybill's compliance posture is a first-class product claim (the
Standards & compliance section of README.md now cites CISA 2026).
The constitution is the internal source of truth for what waybill
promises to conform to; a version drift between the CISA reference
in the constitution and the actual target in the emitters is
exactly the kind of "silent deviation" the constitution's
Governance section prohibits.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

## When to open

- Open AFTER `221-cisa-2026-elements-audit` merges to main.
- Do NOT open from the 221 branch — the constitution change is
  scope-independent and belongs in its own review.
- Do NOT bundle with any other constitution amendment (per the
  constitution's Governance section, each principle change should
  land in a "dedicated PR with a clear rationale").
