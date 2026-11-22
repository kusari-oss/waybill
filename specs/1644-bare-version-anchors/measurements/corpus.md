# T017: public corpus at the final code commit

## Run 45039491162, at `b94a5a19`

- **Dispatch**: `public-corpus.yml` with `-f branch=1644-bare-version-anchors`.
- **Commit**: the summary reads `Scanned commit: b94a5a19…`, the final code commit.
- **Result**: 161 passed, **5 failed**, 166 in total. All 5 are Nix closure targets: `nix-closure-moat`, `-shellcheck`, `-slack-web`, `-slacklinker` and `-openai`. Each drifted in all three formats.

Spec 1644's log-only commit had passed 166 of 166 (`census.md`). It held the `gmp` and `liblzma` CPE rows back for exactly this reason (`control.md`): a row applies to every `pkg:generic/<slug>` component, whichever reader emitted it.

### Every difference, traced

Each `.actual` document from the run's `corpus-emitted-sboms` artifact was compared with its golden by plain `diff`. The 15 diffs (5 targets × 3 formats) have the same content, and differ only in the coverage numbers:

| Format | Lines removed | Lines added | Content |
|---|---|---|---|
| CycloneDX | 1 | 14 | `pkg:generic/gmp@6.3.0` gains `"cpe": "cpe:2.3:a:gmplib:gmp:6.3.0:*:*:*:*:*:*:*"` and its CPE evidence (`waybill curated CPE table`, confidence 0.7); `waybill:cpe-coverage` changes |
| SPDX 2.3 | 1 | 7 | the same component gains a `SECURITY` `cpe23Type` external reference with the same CPE; the coverage annotation changes |
| SPDX 3 | 1 | 7 | the same component gains a `cpe23` external identifier with the same CPE; the coverage annotation changes |

The coverage change is the same in every target: `unaddressable` −1 and `with_candidates` +1. One component, GMP, moved from no CPE to one candidate.

| Target | `unaddressable` | `with_candidates` |
|---|---|---|
| nix-closure-moat | 294 → 293 | 71 → 72 |
| nix-closure-shellcheck | 295 → 294 | 72 → 73 |
| nix-closure-slack-web | 180 → 179 | 55 → 56 |
| nix-closure-slacklinker | 391 → 390 | 611 → 612 |
| nix-closure-openai | 250 → 249 | 54 → 55 |

No component is added or removed, and no other field changes. In every target the component is the closure tier's own `pkg:generic/gmp@6.3.0`, and the cause is T012's `gmp` row. No binary in these targets was versioned by the new anchored scan. These closures' `xz` is emitted as `pkg:generic/xz`, which already had its row, so the new `liblzma` row changes nothing here.

**Judgement**: intended. The CPE is the one the row's NVD evidence supports (`gmplib:gmp`, 35 CVEs, `nvd-vendor-cves-2026-11-22.tsv`), applied to a component already named and versioned `gmp@6.3.0`. The goldens are regenerated below. The diff is not loosened.

## Regeneration, run 45060082406 at `4112ca3a`

- **Dispatch**: `public-corpus.yml` with `-f branch=1644-bare-version-anchors -f regen_goldens=true`.
- **Commit**: `Scanned commit: 4112ca3a…`. The only change since `b94a5a19` is the module's doc comment, so no code differs.
- **Result**: 166 passed. The run lists exactly 15 changed goldens: the five Nix closure targets, three formats each.
- **Copied into the tree**: all 67 files from `corpus-goldens-regen`. `git status` shows exactly those 15 files modified. Each is byte-identical (`cmp`) to the `.actual` document traced above, so what is committed is what was judged.
