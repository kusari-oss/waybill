# Contract — offline-derived distribution references (US2)

**Feature**: 776-component-source-refs
**Status**: Complete
**Date**: 2026-09-05

Contract for extending `scan_fs/mod.rs::external_refs_from_purl`, a pure function of the PURL and the component's annotations.

---

## Contract 1 — Purity preserved (FR-009)

**Post-milestone**: the function remains a pure function of its inputs — no network, no filesystem, no clock. This is what lets US2 work under `--offline`, where US1 is inert by construction.

**Verification**: code review; unit tests call it directly with constructed PURLs and no environment.

---

## Contract 2 — Derive only when the URL is fully determined (FR-009, FR-010)

**Post-milestone**: a `distribution` reference is emitted only when the registry's download URL is completely determined by the PURL's name, version, and namespace. When it is not — because the scheme embeds a content hash, an upload-time path segment, or any value requiring a registry lookup — **nothing is emitted**.

**Binding constraint**: an ecosystem arm MUST NOT be added on the strength of a pattern that appears to work on sampled packages. Each candidate must be verified against its registry's documented URL scheme before the arm is written. A URL that resolves for common packages but not for edge cases is a fabricated reference under Principle IX.

**Verification**: per-ecosystem unit tests over the arms added, including at least one package whose name requires encoding.

---

## Contract 3 — Missing version yields nothing (FR-010)

**Post-milestone**: a PURL without a version produces no distribution reference, because the URL cannot be formed correctly.

**Verification**: unit test over a versionless PURL asserting no distribution reference.

---

## Contract 4 — Additive to existing references (FR-011)

**Post-milestone**: the registry landing pages currently emitted as `website` for `cargo`, `nuget`, and nested-jar `maven` are **preserved**. The distribution reference is added alongside.

This is the correction the measurement motivated: those `website` references are a different claim from a distribution URL and do not answer the source-provenance question — which is why one fixture measured near-zero source coverage while carrying 61 references. The fix adds a correct kind; it does not swap one for another.

**Verification**: fixture comparison asserting the pre-existing `website` references remain and the `distribution` references are new.

---

## Contract 5 — Correct encoding for namespaced names (edge case)

**Post-milestone**: PURLs whose name or namespace requires percent-encoding (scoped packages, group-qualified coordinates) produce correctly-formed URLs rather than naive concatenations.

**Verification**: unit test over at least one scoped/namespaced coordinate per arm added.

---

## Contract 6 — No new operator surface or dependencies (FR-014, FR-015)

**Post-milestone**: no flags, no environment variables, no new dependencies. URL construction uses formatting and encoding facilities already present.

**Verification**: `git diff` shows no manifest or lockfile change; CLI help output unchanged.

---

## Contract 7 — Parity extractors become meaningful (research R6)

**Post-milestone**: catalog rows A9 (homepage), A10 (vcs), and A11 (distribution) and their existing parity extractors begin exercising real data across CycloneDX, SPDX 2.3, and SPDX 3, where today they compare empty against empty.

**This is the most likely place for the milestone to fail first, and that is intended.** Any asymmetry in how the three formats represent the same reference surfaces here as a parity failure. A failure in this suite should be investigated as a genuine cross-format mapping discrepancy rather than treated as unexpected.

**Verification**: the existing `holistic_parity` suite passes with references populated.
