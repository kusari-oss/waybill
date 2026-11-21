# T029: public corpus and quality corpus (SC-004, SC-005)

The final-code measurements ran at branch `ebbffb21`. Since then the S1 fold's redundant C56 skip has been removed. Mutation M9 shows that removal changes no output, so these results apply to the final code.

## Quality corpus (SC-005)

**Runs.** `quality-corpus.yml` on the branch (run 44918354981, `waybill_sha` `ebbffb21…`) and on `main` (run 44918357352, `waybill_sha` `4eec5a4c…`). Every job's conclusion on both runs: `Measure SBOM quality corpus` `success`, `Report quality-corpus failure` `skipped`. `violations` is `[]` on both.

**Metrics.** Across all 18 targets these fields are identical:
- `status`, `pkgs`, `files`, `edges`;
- `nodes_with_out_edges`, `max_depth`, `graph_completeness`, `flat`;
- the sbomqs score.

Timing fields were excluded. The only differences are in `sbom_bytes`:

| Target | `main` | branch | Δ |
|---|---|---|---|
| cmake-nlohmann-json | 57,718 | 57,832 | +114 |
| python-pytorch | 605,073 | 605,529 | +456 (4 × 114) |

**Component by component.** The quality-corpus run uploads no SBOMs. The three targets, and TheRock, were traced from `census-merges`, which scans the same pinned trees and uploads the CycloneDX output:
- run 44918370186 at the branch;
- run 44918372804 at `main`.

Each job's `refs.txt` names its waybill SHA, and all 8 jobs succeeded. For every component, including `metadata.component`, the property maps were compared:

| Target | components (`main` / branch) | changed properties |
|---|---|---|
| cmake-nlohmann-json | 25 / 25 | `pkg:generic/nlohmann_json@3.12.0` gains C56 `["cmake-find-package"]` |
| python-pytorch | 276 / 276 | C56 `["cmake-find-package"]` on `pkg:github/pytorch/gloo@74cc005a…`, `pkg:github/pybind/pybind11@97bf890d…`, `pkg:github/protocolbuffers/protobuf@f0dc78d7…`, `pkg:generic/torch` |
| cpp-mongo | 418 / 418 | none |
| TheRock (submodules) | 2,948 / 2,948 | 84 components gain C56; nothing else changes |

TheRock's 84 C56 values:

| C56 | components |
|---|---|
| `["cmake-find-package"]` | 63 |
| `["cmake-pkg-check-modules"]` | 7 |
| `["cmake-fetchcontent-url"]` | 6 |
| `["cmake-find-package","cmake-pkg-check-modules"]` | 2 |
| `["cmake-find-package","cmake-project"]` | 2 |
| `["cmake-externalproject"]` | 1 |
| `["cmake-externalproject","cmake-fetchcontent-git"]` | 1 |
| `["cmake-externalproject","cmake-project"]` | 1 |
| `["cmake-fetchcontent-git"]` | 1 |

74 of the 84 list a package-search mechanism.

**Every change is a C56 addition (FR-001, FR-004).** No C55, PURL, version or other property changes, and no component appears or disappears (FR-003, FR-010).

**Census versus outcome.** The census predicted 88 recording merge groups on TheRock; 84 components carry C56. A merge group is (site, survivor), so a component that survives merges at two sites is two groups and one C56. The census's 1 and 4 groups on nlohmann-json and pytorch match the 1 and 4 changed components exactly.

## Public corpus (SC-004)

Run 44918352823 of `public-corpus.yml`, `regen_goldens=true`, on the branch. Jobs:
- `Run public corpus (1639-also-detected-via)`: success;
- `Report corpus failure`: skipped (its condition is `failure() && github.event_name == 'schedule'`).

The log names the scanned commit three times as ebbffb21: at checkout, at "Resolve the scanned commit", and as `SCANNED_SHA` in the summary.

The `corpus-goldens-regen` artifact holds 67 files for 21 targets. Each was compared with `cmp` against its committed golden: **67 identical, 0 changed, 0 missing.** No golden changes, so none is committed.

As a control that the regenerated goldens are real output of the branch, C55 appears in 18 of the 67 files. C56, C242 and the `waybill-source-mechanism` methods key appear in none. No public-corpus target has a merge group with more than one mechanism, so FR-012's byte-identity holds on all 21 (SC-004).

ebbffb21 precedes the fold change in b57f0c36 (`therock.md`). That change only affects a C56 key already present on a merge member, and no public-corpus golden carries one.
