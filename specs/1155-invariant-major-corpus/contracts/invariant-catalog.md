# Contract — The Invariant Catalog

**Artifact**: `docs/reference/sbom-invariants.md`
**Consumer**: `corpus_harness_195::invariants::catalog`
**Gate**: a hermetic `cargo test` unit; no network, no corpus run.

The catalog is the human-readable source of truth for what the corpus
checks. The registry in code is the machine side. Neither may contain a
row the other lacks.

---

## Row format

One markdown table, one row per invariant, in ascending number order
including retired rows.

```markdown
| ID | Statement | Precondition | Kinds | Status |
|---|---|---|---|---|
| I1 | No dependency edge targets a file-tier component | document contains at least one file-tier component | `file-tier-dependency-edge` | Active |
| I2 | `(name, version, type)` identifies at most one component | at least two components | `duplicate-file-identity`, `duplicate-library-identity` | Active |
| I8 | (retired — superseded by I11) | — | — | Retired |
```

| Column | Rules |
|---|---|
| `ID` | `I<n>`, `n` a positive integer, ascending, no gaps except retirements. |
| `Statement` | One sentence, present tense, stating the property that must hold. Not a description of the check. |
| `Precondition` | What must be true of a document for the invariant to be meaningful. `—` only for retired rows. |
| `Kinds` | Comma-separated backticked violation kinds. Non-empty for active rows. |
| `Status` | `Active` or `Retired`. |

Each row is followed by prose under an `### I<n>` heading giving the
diagnostic — the class of defect it catches and what to do when it
fires. This is the `suggested_action` text and is the most valuable part
of the existing per-target layer (FR-005); it carries over rather than
being reinvented.

---

## Numbering rules

Enforced by the gate, derived from FR-001a/b/c.

1. **Append-only.** A new invariant takes `max(n) + 1`. Numbers are never reassigned when rows are added, removed or reordered.
2. **Retire, don't edit.** Refining a statement's wording keeps the number. Changing *what the row means* retires it and allocates a new one. The test of "changed meaning" is whether an existing declaration against it would now be accepting a different defect.
3. **Never reuse.** A retired number stays in the catalog, marked `Retired`, with a pointer to its successor if it has one. It may never be reallocated.
4. **Retired rows have no checker.** The registry must not contain an entry for a retired number.

---

## The bidirectional gate

Mirrors `every_catalog_row_has_an_extractor`
(`waybill-cli/src/parity/extractors/mod.rs:922`), which already asserts
both directions. FR-008 requires the first; the second is adopted
because a checker with no catalog row is an invariant nobody can
discover or review.

| Condition | Result |
|---|---|
| Active catalog row with no registry entry | **fail** — names the missing IDs |
| Registry entry with no catalog row | **fail** — names the orphan IDs |
| Registry entry for a `Retired` row | **fail** |
| Catalog row whose `Kinds` omit a kind the checker can emit | **fail** |
| Checker emitting a kind absent from its row's `Kinds` | **fail** at evaluation time |
| Duplicate or non-ascending IDs | **fail** |
| Active row with empty `Kinds` or empty `Precondition` | **fail** — FR-012a, FR-006 |
| Active row with no `### I<n>` diagnostic section | **fail** — FR-005 |

The kind-set agreement in rows 4 and 5 is what keeps FR-012's pinning
honest: if a checker can emit a kind the catalog never listed, a
reviewer approving a declaration cannot know what they are accepting.

---

## What the gate does not check

Stated so the boundary is deliberate rather than assumed:

- **That a statement is true of a well-formed SBOM.** The catalog says what we believe; the corpus run says whether it holds. A wrong statement produces red cells, not a gate failure.
- **That kinds are well-chosen.** FR-012a forbids an unclassifiable checker, but nothing mechanical can tell that a kind is too coarse. `I2` is the known-risk case (research R4) and needs human review.
- **Applicability.** FR-006a's "at least one applicable target" needs a real corpus run and is enforced in the lane, not here.
