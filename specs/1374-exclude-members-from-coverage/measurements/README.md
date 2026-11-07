# Measurements — spec 1374 (issue #1380)

**Date**: 2026-11-07 | **Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at `d069055` | **Pre-change binary**: `7f47ba66`

> **These figures are aarch64-darwin.** The corpus pins x86_64-linux, which
> resolves a different closure carrying **1,087** members to darwin's 539.
> Quoting one against the other is the error milestone 1366 recorded once.

---

## The delivered row

```json
{"schema":"waybill-cpe-coverage/v3","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":194,
 "excluded_members":539,"excluded_files":0}
```

**78 + 166 + 194 + 539 + 0 = 977**, the emitted component count — exactly the
figures computed from the document before any code was written.
`partition.py` now prints **AGREES** where it printed `DISAGREES`.

| | before | after |
| --- | --- | --- |
| `with_candidates` | 78 | **78** — unchanged, as FR-004 requires |
| `addressable_without_cpe` | 166 | **166** — unchanged *on this host* |
| `unaddressable` | **733** | **194** |
| `excluded_members` | — | **539** |
| `excluded_files` | — | **0** |

## The three leak controls

| check | result |
| --- | --- |
| components whose `{purl, cpe, cpes}` changed | **0** of 977, compared as a set |
| differing JSON paths, whole document | **1** — the C209 value |
| members carrying a CPE | **0** of 539 — the structural claim behind FR-004 |

## T013a — the split, and its 10 callers

`fill_missing_cpes` now synthesizes and returns `()`; `cpe_coverage(&[..])`
counts. Ten unit-test call sites read a coverage value back from the old
signature; each became two lines and **kept its original assertion
unchanged**. All 148 `scan_cmd` tests pass, which is the evidence the split
preserved meaning rather than merely compiling.

`/speckit.analyze` raised this as CRITICAL before implementation, and it was
right to: a signature break invites the quickest repair rather than the one
that keeps each test meaning what it meant.

## T025 — the mutation check

| mutation | result |
| --- | --- |
| the member test made a no-op | **3 failed** |
| the file-tier test made a no-op | **2 failed** |
| exclusions moved **after** the CPE test | **1 failed** |
| the partition assertion deleted | ok — **survived** |
| the counting loop drops a bucket | **failed** |
| members counted into `unaddressable` (**sum still holds**) | **failed** |

**The fourth surviving is expected here and was checked rather than
assumed.** Milestone 1373 found the same mutation survive, and that *was* a
gap: nothing else compared the sum. Here `m1374_t014` does its own
`total() == len()` comparison, so the invariant is covered by a test rather
than by an assertion no test can distinguish from its own absence. The last
two mutations prove it: one drops a component from every bucket, the other is
**this milestone's own defect reintroduced** — members counted as
`unaddressable`, with the sum still closing — and both go red.

Every mutation was reverted with `cp` from a backup. Milestone 1373 used
`git checkout` on an uncommitted file for one of them, which deleted the work
under test while the gate still passed.

---

## T030 / T031 — the corpus refresh

Run `42382685849`, dispatched against this branch. **The scanned commit it
reports, `1b220839`, is this branch's tip** — checked, not assumed, because a
`workflow_dispatch` run's `github.sha` is the workflow's own commit.

Accounted for from the artifact rather than the run summary (#1286): 51
committed files, 51 in the artifact, none new, none missing. 16 targets show
**exactly 6 changed lines each, all C209, zero other**; those are taken.
`image-postgres16` shows 896 other changed lines — #1372's drift — and stays
excluded.

### The linux figures, which are not darwin's

| | darwin | linux |
| --- | --- | --- |
| `with_candidates` | 78 | 6 |
| `addressable_without_cpe` | 166 | **162** (was 163) |
| `unaddressable` | 194 | **360** (was 1,446) |
| `excluded_members` | 539 | **1,087** |
| `excluded_files` | 0 | 0 |
| sum = components | 977 | 1,615 |

## T021 — only one count moved that was not supposed to, and by exactly 1

Across all 17 targets:

- **`with_candidates` moved on none of them**, which is the structural claim
  behind FR-004 and is now measured rather than argued.
- **`addressable_without_cpe` moved on exactly one**, `nix-closure-moat`, by
  exactly **−1**. That is `pkg:hackage/time`, role `fragment` — the single
  member carrying an OSV-addressable PURL, and the component that falsified
  the spec's original FR-004.
- `unaddressable` fell by 1,086 there.

Any other movement would have been a finding. There was none.

## T018 — the sum closes on 15 of 17, and the two exceptions are structural

**Both are targets whose golden is not the whole document.**

`image-postgres16` carries no C209 at all (#1372).

`pants-example-javascript` reported `304 ≠ 303` and **that is not a defect**.
Its golden is filtered to `pkg:npm/*` only by
`corpus_harness_195/js_filter.rs` (#760, feature 675 FR-008) to stay under a
size budget: the full CDX is ~570 KB. The C209 row counts the whole document
— 302 npm components, the root, and one file-tier entry — while the golden
retains 303 of them. The row is right; the golden is a subset.

**My predicted figure for that target was wrong, and in a way worth
recording.** `contracts/annotations.md` said `excluded_files: 0` there,
because I computed the prediction *from the filtered golden* as though it were
the document. The real document has 1. A filtered artifact read as a complete
one gives a confident wrong answer, and only running the check found it.

SC-006 is amended to name both exceptions rather than demand something
impossible of them.
