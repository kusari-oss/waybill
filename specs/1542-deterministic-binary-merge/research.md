# Research: the same build gives the same answer, scan after scan

**Evidence:** `measurements/trace.md` (FR-001), from a synthetic reproduction (`measurements/probe_repro.rs`) and the SDK documents of `measure-full` runs 44189121691 and 44193745935.

## R1. Different files with one name stay apart (FR-003)

**Decision.** The deduplicator adds a component's `file-sha256` PURL qualifier to its group key. Components that each name one file's bytes merge only when name **and** content hash match.

**Rationale.** The trace found the cause: two different libraries with one file name merged into one, and the loser's identifications kept a parent that no longer existed. Adding the qualifier, alone, made the reproduction emit both file components with every identification nested, in 12 of 12 scans.

**Why only that qualifier.** Milestone 1064 kept the full PURL out of the key, because it would split qualifier-only variants (`deb ?arch=`) that are meant to merge. `file-sha256` is different in kind: it names bytes, and two different byte sequences are two files. Every other qualifier is unchanged.

**Alternatives considered:**
- *Union the identifications into the surviving file component.* That reports one library's contents under another's PURL and hash: a false statement about both files.
- *Re-point orphaned identifications at the survivor.* The same falsehood.

## R2. Containment edges independent of component order (FR-002, FR-005)

**Decision.** In `graph_completeness::add_containment_edges`, rule R2 (a nested child's own dependency edges leave its node key) reads from a snapshot of the dependency edges, taken before any containment edge is added.

**Rationale.** R2 read `edges[purl]` in the same loop as R3 wrote to it. A nested copy visited after another same-PURL copy therefore gained links to its siblings, the detached one included. The deduplicator's output order varies between processes, so reachability varied while the document did not. With the snapshot, the reproduction's orphan count was identical in 12 of 12 scans.

**This is spec 1541's code.** The defect landed in #1603, and its tests passed because they never scanned twice.

## R3. The deduplicator's output order and survivor (FR-002)

**Decision.**
- The deduplicator returns its groups in a fixed order: the order each group's first member arrived in.
- Within a group, the survivor is chosen by confidence, with ties broken by PURL string and then by first source path.

**Rationale.** Both currently follow `HashMap` iteration and input order. R2 removes the one consumer the trace showed to be order-sensitive, but FR-002 asks for order independence, not for every consumer to be audited.

**First-arrival order** keeps today's order wherever input order is already fixed. A sort by PURL would reorder every document's components, a golden churn with no information in it.

*Inference, to be measured by the audit (R5):* whether any emitted document's component order follows the deduplicator's output directly.

## R4. SPDX 2.3 identity (FR-004)

**Decision.** No SPDX 2.3 change, unless the audit shows one is still needed.

**Rationale.** SPDX 2.3 folded the detached `gotcha` into the same-PURL package nested elsewhere. Once R1 keeps every identification under its own file, the detached copy no longer exists, and the formats report one component set. The contract checks this (D-5), and the SDK run verifies it (SC-003).

## R5. The determinism audit (FR-006)

**Decision.** Measured, not assumed, in three places:

| where | how |
|---|---|
| **Synthetic** | `waybill-cli/tests/scan_determinism.rs` scans each contract fixture at least 20 times and compares the masked documents. |
| **TheRock SDK** | `measure-full` keeps every scan's documents, not only the last, and the probe compares them masked. A fixture-repository workflow change. |
| **Public corpus** | a `WAYBILL_CORPUS_REPEAT` setting in the corpus harness scans each target a second time and compares the two masked documents. It is dispatched once in CI, at the branch and at `main`. |

**Masking.** Serial number and document timestamps, and whatever the corpus golden comparison already masks (`layer2_golden`).

**Scope (clarification).** Variation in the binary walk, analysis or merge path is fixed here; anything else is filed as its own issue and recorded.

## R6. Cost

*Inference.* One more key field, a snapshot clone of the adjacency map (once per document), and a deterministic sort of groups. SC-007 is measured by interleaved `measure-full` and `measure-source`.
