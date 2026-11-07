# Measurements — spec 1376 (issue #1385)

**Date**: 2026-11-07 | **Host**: aarch64-darwin | **waybill**: `6c13356b`
**Subject**: `slack-web` @ `7b46273`, a published Hackage library whose
single main module makes it the shape the corpus cannot produce.

---

## The row is right in all three formats

```
row (identical in CDX, SPDX 2.3, SPDX 3): 200
```

| format | containers | total |
| --- | --- | --- |
| CycloneDX | 199 `components[]` + 1 `metadata.component` | **200** |
| SPDX 2.3 | 200 `packages[]` | **200** |
| SPDX 3 | 197 `Package` + 3 `File` | **200** |

Nothing is missing from any document. The count is correct and correct
everywhere.

## What fails is the documented check

`.components | length` returns **199**, because CDX writes the elected root
to `metadata.component`. A consumer following milestone 1374's instruction
sees a mismatch and concludes the row is broken.

SPDX 3 fails the analogous check for a different reason: file-tier entries
are `File` nodes, so counting `Package` nodes is short by exactly
`excluded_files` (3 here).

## The control: one flag, same project, same binary

```
without --root-name   root=pkg:hackage/slack-web@2.4.0.0       row=200  components[]=199
with    --root-name   root=pkg:generic/slack-web-harness@0.0.0 row=200  components[]=200
```

All 17 corpus targets pass `--root-name`, so every one has a synthetic root:

```
go-cobra          pkg:generic/go-cobra@a655097
nix-closure-moat  pkg:generic/nix-closure-moat@d069055
python-flask      pkg:generic/python-flask@80be49b
```

**The corpus is structurally incapable of producing a real root**, so no
number of added targets would have caught this.

## The rule behind it

`root_selector::select_root`, documented at `cyclonedx/metadata.rs:716`:
exactly one main-module → promote it to `metadata.component`; several → use a
synthetic placeholder and leave all N in `components[]`.

| case | root | in `components[]`? | `.components\|length` check |
| --- | --- | --- | --- |
| synthetic (`--root-name`, or no main module) | not a resolved component | no | agrees |
| **one main-module, promoted** | real | **no** | **short by 1** |
| several main-modules | real | yes | agrees |

## A correction to my own issue

#1385 was filed as a **counting defect** and as *"the third instance of one
shape"* — the counted slice and the emitted array being different sets.

That framing is right for milestone 1373 (file-tier components were outside
the counted slice, a real under-count) and for 1374 (closure members were
inside it and should not have been, a real over-count). It is **wrong** here:
the slice and the emitted set are the same size, and only the container one
component is written to differs.

Three symptoms that look alike are not three instances of one cause. The
issue has been corrected and retitled; this file records the error rather
than only the conclusion, because the pattern-matching that produced it is
the part worth distrusting next time.
