# Data model: spec 1644

All in-process, per binary. Nothing persists beyond the emitted document.

## Whole string (FR-004, amended by the census)

The printable tail of a NUL-terminated segment: preceded by NUL, by the region's start, or by a byte that is not printable text (`0x20`–`0x7E`, tab, LF, CR). The census records which, as `preceding` (`nul`, `start` or `binary`).

## CuratedLibrary (extended)

Three new variants. Their slugs are the PURL names (FR-007, research R5).

| Variant | Slug | Boundary | Anchor | Candidate shape (whole NUL-bounded string) | Emitted version |
|---|---|---|---|---|---|
| `Bzip2` | `bzip2` | `WholeString` | text `bzip2/libbzip2`, substring anywhere in the region | `X.Y.Z, DD-Mon-YYYY` | `X.Y.Z` |
| `Gmp` | `gmp` | `WholeString` | text `GNU MP:`, substring anywhere in the region | `X.Y.Z` | `X.Y.Z` |
| `Liblzma` | `liblzma` | `WholeString` | defined symbol `lzma_version_string` or `lzma_version_number` | `X.Y.Z` or `X.Y.Z` + `alpha`/`beta` | the whole string |

- X, Y and Z are 1–4 ASCII digits each.
- DD is 1–2 digits.
- Mon is one of `Jan`…`Dec`.
- YYYY is 4 digits.

## Boundary (extended)

`Anchored | NulOnly | WholeString`. `WholeString` libraries never match through `pattern_at`. They are reached only through `scan_anchored`. `CuratedLibrary::boundary()` stays an exhaustive match.

## BinaryScan.anchor_symbols

`Vec<String>`: the names from `version_strings::ANCHOR_SYMBOLS` that the binary **defines**:
- ELF: dynamic symbols that are not undefined;
- Mach-O: global symbols that are not undefined, with one leading `_` stripped;
- PE: export names.

The field is empty for every other binary and is never emitted.

## AnchoredVerdict (per binary × anchored library)

| Verdict | Condition | Emits |
|---|---|---|
| `Versioned(v)` | anchor present, and exactly one candidate after FR-005a's removal | `pkg:generic/<slug>@<v>`, the same as any curated match |
| `Ambiguous(c1, c2, …)` | anchor present, and two or more candidates after removal | nothing |
| `AmbiguousShared(shared…)` | anchor present, and every candidate is also accepted by another library anchored in the same binary | nothing |
| `NoCandidate` | anchor present, and no candidate | nothing |
| (not considered) | no anchor | nothing; no census event |

Order of evaluation:
1. Collect the anchored libraries.
2. Collect each one's candidates (distinct strings).
3. Remove from every library any candidate accepted by another anchored library.
4. Decide the verdict.

A library with `Ambiguous` before removal stays `Ambiguous` if two or more candidates remain after it. If removal leaves exactly one, that one is the version. If removal leaves none, and the only candidates were shared, the verdict is `AmbiguousShared`.

## Census event (`waybill::version_census`, DEBUG)

One `anchored` event per binary × anchored library, emitted only when `tracing::enabled!`. Its fields are in `contracts/wire.md`. The existing per-pattern `hit` events are unchanged.
