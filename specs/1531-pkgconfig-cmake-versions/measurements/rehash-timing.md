# T030: re-hashing a matched shared library for evidence

**Fixture.** `opt/sdk/lib/libwaybill-fixture-a.so.1.2.3` is a 200 MiB sparse
file with a 64-byte ELF64 `ET_DYN` header. Arm `with_pc` adds
`opt/sdk/lib/pkgconfig/waybill-fixture-a.pc`, so the library is listed in
the `.pc` component's evidence. That listing hashes the file again through
`manifest_occurrence` (research R9). Arm `without_pc` has no `.pc`.

**Method.** Debug build. Each arm got one warm-up run, then 5 interleaved
rounds with the arm order alternating each round.

| Arm | Min | Median | Max | Runs (s) |
|---|---|---|---|---|
| with_pc | 12.842 | 15.928 | 40.248 | 12.878, 40.248, 27.492, 12.842, 15.928 |
| without_pc | 12.826 | 23.332 | 41.630 | 12.826, 32.395, 23.332, 22.562, 41.630 |

**Reading.**
- The variance is large in both arms, from about 12.8 s to 41 s.
- The minimums are equal to within 16 ms, and `with_pc`'s median is lower,
  not higher.
- So a second hash of a 200 MiB file is not measurable against the binary
  scan's own cost of the same file, and the task's 1.5× trigger for taking
  the hash from the binary component's `file-sha256` qualifier is not met.
- The evidence is in place: the `.pc` component's `evidence.occurrences`
  lists both the `.so` and the `.pc`.

This measurement is a debug build on one host. A release-build measurement
on a real SDK is SC-006's CI run (T036).
