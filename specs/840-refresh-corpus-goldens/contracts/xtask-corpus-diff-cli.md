# Contract: `xtask corpus-diff`

Feature `840-refresh-corpus-goldens` · satisfies FR-004, FR-005, FR-013a

A **review-time** normaliser. It exists so a human can read a
golden diff spanning 147 merges. It is not part of the gate and never
writes to committed goldens — changing what is stored would change what
the lane asserts.

## C-1 — Invocation

```
xtask corpus-diff --old <path> --new <path> [--format cdx|spdx-2.3|spdx-3]
xtask corpus-diff --target <name> --old-ref <git-ref>
```

- **C-1.1** Compares two golden files, or one target's goldens against a git ref.
- **C-1.2** Writes the normalised diff to stdout. Exit 0 whether or not differences exist — this is a reading tool, not a gate. A non-zero exit would invite someone to wire it into CI as a second gate, which it is not.
- **C-1.3** Exits non-zero only on operational failure: unreadable input, unparseable JSON, unknown target.

## C-2 — Normalisation

- **C-2.1** Unordered collections MUST be sorted by a stable key before comparison. SPDX 3 `@graph` is the motivating case: its order is not guaranteed stable across runs and reordering otherwise presents as every element changing.
- **C-2.2** Normalisation MUST be applied identically to both sides. Asymmetric normalisation manufactures differences.
- **C-2.3** The tool MUST NOT write to any file under `waybill-cli/tests/fixtures/`. Read-only with respect to goldens, enforced by not opening them for writing.
- **C-2.4** The tool MUST NOT re-apply the harness's existing masking. Goldens are stored already masked (`layer2_golden.rs:51`); masking again would risk diverging from what the gate compares.
- **C-2.5** Sorting MUST be total and deterministic. A comparator that ties on equal keys leaves order input-dependent, which reintroduces the problem it exists to solve.

## C-3 — Output

- **C-3.1** Output MUST identify target and format, so a diff pasted into a PR is attributable without its invocation.
- **C-3.2** Output SHOULD group repeated shapes of change so a reviewer sees "N components gained X" rather than N separate hunks. This is what makes 147 merges of drift tractable.
- **C-3.3** Changes matching no group MUST be shown individually and MUST NOT be summarised away. These are the FR-007 cases — the whole point.

## C-4 — What it must not become

- **C-4.1** Not a gate. The lane's byte-identity comparison stays authoritative.
- **C-4.2** Not a golden writer. Regeneration happens only through the existing CI dispatch (FR-002, FR-003).
- **C-4.3** Not a masker. If a new non-deterministic field appears, it belongs in the harness's `mask_nondeterministic` where the gate will honour it — not here, where only reviewers would.

## C-5 — Verification

- **C-5.1** Given two goldens differing only in array order, output MUST be empty.
- **C-5.2** Given two goldens differing in one component's licence, that difference MUST appear.
- **C-5.3** Given a golden compared against itself, output MUST be empty.
- **C-5.4** After any run, committed goldens MUST be byte-identical to before. This is the check that catches C-2.3 being violated by accident.
