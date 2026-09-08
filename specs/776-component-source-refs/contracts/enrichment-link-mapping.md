# Contract — enrichment link → reference mapping (US1)

**Feature**: 776-component-source-refs
**Status**: Complete
**Date**: 2026-09-05

Contract for consuming the enrichment payload's `links[]` inside `depsdev_source.rs::apply_version_info` — the function that already applies the license half of the same response.

---

## Contract 1 — No additional network request (FR-007)

**Post-milestone**: satisfying US1 issues zero new HTTP requests. The links consumed come from the `VersionInfo` payload the scan already fetches for license enrichment, and the existing per-scan response cache is reused unchanged.

**Verification**: code review — no new client call in the mapping path. Empirically, scan wall time is within 3% of baseline (SC-006); a per-component network call on a 369-component fixture could not hide inside that budget.

---

## Contract 2 — Label mapping is total, closed, and label-driven (FR-001, FR-002, FR-002a, FR-003)

**Post-milestone**: exactly five labels map to natively-defined CycloneDX types.

| Label | Kind |
|---|---|
| `SOURCE_REPO` | `vcs` |
| `ISSUE_TRACKER` | `issue-tracker` |
| `DOCUMENTATION` | `documentation` |
| `HOMEPAGE` | `website` |
| `ATTESTATION` | `attestation` |
| `ORIGIN` | *(unmapped — skipped and counted)* |
| anything else | *(unmapped — skipped and counted)* |

**Binding constraints**:
- The kind is chosen from the **label only**. The URL's shape MUST NOT influence it — a `HOMEPAGE` pointing at a repository host is still `website`. Inferring kind from URL shape is the guess FR-003 exists to prevent.
- `ORIGIN` is not special-cased into silence; it is skipped *and counted*, so the summary reflects reality.
- An unmapped label MUST NOT fail the scan and MUST NOT emit per-occurrence output.

**Verification**: unit tests over each mapped label, over `ORIGIN`, and over a synthetic unknown label; plus a test asserting a repository-host URL carried under `HOMEPAGE` still yields `website`.

---

## Contract 3 — Every emitted kind is natively defined (FR-005, SC-009)

**Post-milestone**: all six kinds this milestone can emit (`vcs`, `issue-tracker`, `documentation`, `website`, `attestation`, `distribution`) are members of the CycloneDX 1.6 `externalReference.type` enum. **No `waybill:*` property is introduced for source-provenance information.**

**Verification**: a test asserting every kind the mapping can produce is in the allowed set. Principle V's bullet-5 audit is satisfied by construction — the native construct is not merely preferred here, it is the only construct used.

---

## Contract 4 — Malformed and empty URLs are omitted (FR-004, NFR-002)

**Post-milestone**: a link whose URL is empty or not a well-formed absolute URL produces no reference, increments the malformed-skip counter, and leaves the component otherwise intact. A component with entirely malformed enrichment metadata is still emitted — without enrichment-derived references.

**Verification**: unit tests over empty string, relative path, and non-absolute forms; plus a test asserting the component survives and its other references are unaffected.

---

## Contract 5 — Deduplication on `(kind, url)` (FR-006)

**Post-milestone**: two references with the same kind *and* the same URL collapse to one. Two references with the same URL under *different* kinds are both retained — they are distinct claims a consumer filters on independently.

**Verification**: unit tests for the duplicate-pair case and the same-URL-different-kind case, asserting collapse in the first and retention in the second.

---

## Contract 6 — Deterministic ordering (FR-013, SC-005)

**Post-milestone**: references are emitted in a stable order derived from `(kind, url)`, not from upstream array order, which is not contractually stable.

**Verification**: SC-005 double-run byte-identity, plus a unit test feeding shuffled input and asserting identical output ordering.

---

## Contract 7 — Inert when enrichment is unavailable (FR-008)

**Post-milestone**: with enrichment disabled or the service unreachable, no enrichment-derived reference appears and the scan completes normally. Output is otherwise unchanged from pre-milestone.

**Verification**: an offline scan produces byte-identical output to the pre-milestone binary for the enrichment-derived portion; US2's derived references are the only additions on that path.

---

## Contract 8 — Existing and operator-supplied references preserved (FR-011, FR-012)

**Post-milestone**: references produced today — including the registry landing pages emitted for `cargo` / `nuget` / `maven` — remain. Operator-supplied references from the existing supplement mechanism pass through unmodified and un-reordered.

**Verification**: fixture comparison showing pre-existing references still present; a supplement-path test asserting operator entries survive.

---

## Contract 9 — Aggregate summary, emitted once (FR-014a, FR-014b, SC-009a)

**Post-milestone**: one summary per scan reporting references emitted per kind, links skipped as unmapped, and links skipped as malformed — the two skip counts distinct.

**Binding constraints**:
- Exactly once per scan, regardless of component count. Never per-component, never per-link.
- Per-kind emitted counts MUST equal the references present in the emitted document. The summary reports what happened; it is not an independent estimate.
- The skip counters MUST remain separate: a rising unmapped count means the upstream vocabulary moved; a rising malformed count means upstream data quality degraded. They call for opposite responses.

**Verification**: a test over a fixture mixing mapped, unmapped, and malformed links, asserting the summary appears once and its per-kind counts match the emitted document.
