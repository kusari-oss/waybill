# Feature Specification: C209 promises a sum a consumer cannot check

**Feature Branch**: `1376-count-emitted-components`
**Created**: 2026-11-07
**Status**: Draft
**Issue**: #1385

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1376**; it fixes issue **#1385**.

## Summary

Milestone 1374 gave `waybill:cpe-coverage` (C209) five counts and this
promise:

> the five counts … sum to the emitted component count … **against the
> document as it stands**, with no filtering rule the reader has to know

**The counts are right. The promise is wrong.** Measured on one scan of a
public Haskell library, across all three formats:

| format | shape | total |
| --- | --- | --- |
| CycloneDX | 199 `components[]` **+ 1 `metadata.component`** | **200** |
| SPDX 2.3 | 200 `packages[]` | **200** |
| SPDX 3 | 197 `Package` **+ 3 `File`** | **200** |

Every format emits exactly 200 things and the row says 200. Nothing is
missing from any document. But a consumer who follows the documented check —
`.components | length` — gets **199** and concludes the row is broken.

It is not broken; the instruction is. The promise holds only for a CDX
document whose root is synthetic, which is the only shape the test corpus
produces.

## Why `.components | length` is short

Two independent reasons, neither a defect:

1. **CDX promotes a single main-module to `metadata.component`.** When a scan
   finds exactly one, `root_selector::select_root` elects it and CDX puts it
   in `metadata.component` rather than `components[]` — the correct CDX
   shape. The component still exists; it is written elsewhere.
2. **SPDX 3 emits file-tier entries as `File` nodes, not `Package` nodes.**
   Counting Packages is short by exactly `excluded_files`.

Both are the formats being themselves. The row is one number carried into
three documents that arrange their contents differently.

## Why nothing caught it

All 17 corpus targets pass `--root-name`, which forces a synthetic root and
so the first reason cannot arise. The control is one flag on one project:

```
without --root-name   root=pkg:hackage/slack-web@2.4.0.0      row=200  components[]=199
with    --root-name   root=pkg:generic/slack-web-harness@0.0.0 row=200  components[]=200
```

**The configuration CI always runs is the one in which the promise is true.**
A user scanning their own single-module project gets the other one.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — a consumer checks the row and believes the answer (P1)

Someone reads C209, applies the documented check, and needs the result to
mean what it says — whether their project has one main module or many, and
whichever format they emit.

**Why this priority**: a self-verifiable row was milestone 1374's entire
justification for a fifth and sixth count. A check that fails on correct
documents is worse than no check, because it manufactures distrust in a
number that is right.

**Independent test**: for each of the three formats, apply the documented
expression to a single-main-module project and to a multi-module one; all
six combinations agree with the row.

**Acceptance**
1. **Given** a CDX document whose root is a real component, **When** the
   documented check is applied, **Then** it agrees with the row.
2. **Given** an SPDX 3 document containing file-tier entries, **When** the
   documented check is applied, **Then** it agrees with the row.

### User Story 2 — the corpus can exercise the shape it currently cannot (P2)

**Why this priority**: the promise was false for three milestones and no test
could have found it, because `--root-name` is passed everywhere. Fixing the
wording without fixing that leaves the next claim about document shape
equally unguarded.

**Independent test**: a check exists that exercises a **real** root, and it
fails if the root is dropped from the accounting.

### Edge cases

- **A project with several main modules** — the elected root stays in
  `components[]` alongside its siblings, so CDX needs no adjustment. The
  check must handle both without a special case the reader applies by hand.
- **A document with no file-tier entries** — SPDX 3's `Package` count then
  equals the total, and the check must still be stated the same way.
- **A scan with `--root-name`** — the root is synthetic and is *not* a
  resolved component. It must not be counted, or the row over-reports by one
  in exactly the configuration CI uses.
- **A format added later** — the property must be expressible for it without
  re-deriving which container holds what.

## Requirements *(mandatory)*

- **FR-001**: The documented property MUST be true for every emitted format
  and every project shape, or it MUST state its own preconditions.
- **FR-002**: The counts themselves MUST NOT change. They are correct, and
  this feature exists because the description of them is not.
- **FR-003**: A consumer MUST be able to verify the row from the document
  alone, without knowing which container a given format uses for the root or
  for file-tier entries.
- **FR-004**: The verification expression MUST be given per format where the
  formats genuinely differ, rather than one expression that is true for one
  of them and quietly wrong for the others.
- **FR-005**: The corpus or the test suite MUST exercise a **real** root —
  the shape `--root-name` currently prevents — so this class of claim is
  guarded rather than assumed.
- **FR-006**: No document's content changes. No component is added, removed
  or relocated to make a check easier.

## Success Criteria *(mandatory)*

- **SC-001**: On a single-main-module project with no `--root-name`, the
  documented check agrees with the row in all three formats.
- **SC-002**: On a multi-main-module project, the same checks agree.
- **SC-003**: On a project with file-tier entries, the SPDX 3 check agrees.
- **SC-004**: Byte-for-byte, no emitted document changes.
- **SC-005**: A test fails if the root is omitted from the accounting, and it
  is demonstrated to fail rather than assumed to.
- **SC-006**: The claim and its preconditions appear where a consumer reads
  the row — the format-mapping documentation — not only in a spec directory.

## Assumptions

- `metadata.component` holding a real component is correct CDX and is not
  under review here. Likewise SPDX 3 emitting files as `File` nodes.
- The root elected by `root_selector::select_root` is a resolved component
  when it is not synthetic; a synthetic placeholder is not one and is not
  counted.

## Out of scope

- Changing which component is elected as the root (#1374's selector).
- Changing document shape to make a check easier — notably, emitting the root
  twice in CDX.
- Any change to the five counts, their names, or the schema version.
