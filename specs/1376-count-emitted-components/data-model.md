# Phase 1 data model

**Feature**: `1376-count-emitted-components` | **Issue**: #1385

**No entity changes. No field changes. No schema change.** This feature edits
three sentences and adds one test. The model below exists to name the thing
the sentences were wrong about.

---

## 1. Two populations that are not the same set

| | what it is | size on `slack-web` |
| --- | --- | --- |
| **the counted slice** | the `&[ResolvedComponent]` `cpe_coverage` receives | 200 |
| **the emitted array** | whatever container a given format writes them to | CDX 199, SPDX 2.3 200, SPDX 3 197 + 3 |

They hold the same components. They differ in **arrangement**, and that is
entirely a property of each format:

```text
                    the 200 resolved components
                                │
            ┌───────────────────┼───────────────────┐
            ▼                   ▼                   ▼
      CycloneDX            SPDX 2.3              SPDX 3
  components[]  199      packages[]  200     Package  197
  metadata.component 1                       File       3
            = 200                  = 200            = 200
```

**The row states 200 and is right in all three.** What differs is the
expression a consumer needs to recover it.

## 2. `RootKind` — the distinction a consumer must make in CDX

Not a Rust type; a distinction readable from the document.

| kind | how it appears | counted? | in `components[]`? |
| --- | --- | --- | --- |
| **synthetic** | `pkg:generic/<target>@<rev>`, **no `waybill:*` properties** | no | no |
| **promoted** | a real PURL, carries `waybill:component-role` etc. | yes | no |
| **duplicated** | same, and its `bom-ref` is also in `components[]` | yes | yes |

Produced by `root_selector::select_root`:

| main modules found | result |
| --- | --- |
| none | **synthetic** placeholder |
| exactly one | that module, **promoted** out of `components[]` |
| several | the **milestone-127 ladder** elects one — it is real and **stays** in `components[]` alongside its siblings |

> **The third row is not what `metadata.rs:716`'s comment says.** That comment
> describes the pre-milestone-127 behaviour — "fall through to the
> synthetic-placeholder path" — and milestone 127 replaced it with the ladder
> a few lines below, as the comment itself notes. Measured: live `moat` has
> two main modules and elects `pkg:hackage/moat@0.1`, a **real** root
> carrying `waybill:*` properties and present in `components[]`, with
> `waybill:root-selection-heuristic` reading `repo-root-main-module`.
>
> This document said "synthetic" until `/speckit.analyze` caught it. Writing
> a spec about a stale comment misleading its readers, from a stale comment
> in the same file, is the error worth noticing here.

**The `waybill:*` properties are the discriminator**, and they are already
emitted. A synthetic root is a label, not a component, so it carries none.

## 3. The verification expressions

```text
CycloneDX   len(components[])
              + 1  if  metadata.component has waybill:* properties
                   and its bom-ref ∉ {components[].bom-ref}

SPDX 2.3    len(packages[])

SPDX 3      count(Package nodes) + count(File nodes)
```

### Why each CDX clause is load-bearing

| clause dropped | what breaks |
| --- | --- |
| the `waybill:*` test | every `--root-name` scan over-counts by 1 — including all 17 corpus targets |
| the `bom-ref ∉` test | live `moat`, whose real root is also a sibling, over-counts by 1 |
| the whole adjustment | every single-main-module scan under-counts by 1 — the reported defect |

## 4. What the `debug_assert` actually checks

```rust
debug_assert_eq!(coverage.total(), components.len());
```

This compares the counts to **the slice**. It is the right check for *"no
component was classified twice or not at all"* and it is **silent about the
document**. Its comment currently claims the latter.

**Keep the assertion; correct the comment.** A true assertion wearing a false
description is how this defect survived: every reader of that comment,
including the one who wrote the documentation row, took it as evidence for a
property it never tested.

---

## Not touched

`CpeSynthesisCoverage` and its five fields, `classify_addressability`,
`FeedAddressability`, the schema version, `root_selector::select_root`, every
emitter, and every byte of every emitted document.
