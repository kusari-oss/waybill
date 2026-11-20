# T002 vendored rule

**Sample**: `vendored-sample.tsv`, committed at `13c870a0` **before** this computation. It has 80 projects, 11 labelled vendored.

**Criterion, fixed in research R10 before any count**:
- precision ≥ 0.90 on the labelled sample;
- among the single signals and two-signal conjunctions;
- ties go to higher recall.

| Rule | Marked | True positives | Precision | Recall |
|---|---|---|---|---|
| path-segment | 10 | 8 | 0.80 | 0.73 |
| submodule-boundary | 40 | 7 | 0.17 | 0.64 |
| licence-file | 10 | 4 | 0.40 | 0.36 |
| not-reached | 70 | 8 | 0.11 | 0.73 |
| **path-segment + submodule-boundary** | **4** | **4** | **1.00** | **0.36** |
| path-segment + licence-file | 3 | 3 | 1.00 | 0.27 |
| path-segment + not-reached | 7 | 5 | 0.71 | 0.45 |
| submodule-boundary + licence-file | 5 | 1 | 0.20 | 0.09 |
| submodule-boundary + not-reached | 40 | 7 | 0.17 | 0.64 |
| licence-file + not-reached | 7 | 2 | 0.29 | 0.18 |

**Chosen: `path-segment + submodule-boundary`.** C240 `waybill:vendored = true` is set when both hold. C241 records every signal that holds, either way.

**Caveats, stated rather than corrected after the fact:**
- **Small sample.** The bar is met on 4 marked projects. A precision of 4 of 4 is weak evidence: its lower confidence bound is well below 0.90.
- **Low recall.** Recall is 0.36, and the rule needs a submodule boundary. It cannot fire on a tree scanned without submodules, such as the quality corpus's mongo (abseil, snappy) and pytorch (clog). Those projects carry C241 `path-segment` and no C240.
- **Why `path-segment` alone fails.** At 0.80 it falls short partly because `external` matches pytorch's own `cmake/External/` build glue, and `third_party` matches pytorch's in-tree `third_party/openreg` test backend.
