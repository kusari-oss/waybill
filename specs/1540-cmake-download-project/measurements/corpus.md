# Corpora (T015, SC-006)

## Public corpus: unchanged

| run | waybill | result |
|---|---|---|
| 44149690672 | branch `cc85a5e9` | compare mode, **158 passed, 0 failed**, plus the 3 other tests. The goldens are byte-identical. |
| 44149696681 | `main` `f0133adc` (control, dispatched at the same time) | 158 passed, 0 failed |

No target changes, so no public-corpus target uses DownloadProject or a wrapper whose keyword lists are variables in a way that changes its document. No golden was regenerated.

## Quality corpus: unchanged

| run | waybill | violations |
|---|---|---|
| 44149702431 | branch `cc85a5e9` | none |
| 44149707597 | `main` `f0133adc` (control) | none |

All 18 targets measure identically at both refs: packages, files, edges, nodes with out-edges, maximum depth, graph completeness and sbomqs score.
