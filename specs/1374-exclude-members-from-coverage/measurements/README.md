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
