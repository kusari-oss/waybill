# T020, T021: public and quality corpora (SC-004)

Final source: `49c12ee8`. Runs at `8d76de26` are summarised at the end; they found nothing the final runs do not repeat.

## Public corpus

Run 44959619717 of `public-corpus.yml`, dispatched `--ref main -f branch=1641-cmake-conditional-scope -f regen_goldens=true`. Jobs:
- `Run public corpus (1641-cmake-conditional-scope)`: success;
- `Report corpus failure`: skipped (scheduled runs only).

The run's `headSha` is `main`'s, because it was dispatched from `main`'s workflow. The scanned commit is in the log ("Resolve the scanned commit": `scanned commit: 49c12ee8…`) and in the summary.

The `corpus-goldens-regen` artifact holds 67 files for 21 targets: 66 documents and one README. Each was compared with `cmp` against its committed copy: **67 identical, 0 changed, 0 missing.** The census found no catch-all conditional in the public corpus (`census.md`), so FR-009 predicts exactly this.

## Quality corpus

Runs of `quality-corpus.yml`, both from `main`'s workflow definition:
- **branch:** 44959622891, `-f branch=1641-cmake-conditional-scope`, waybill `49c12ee8`;
- **main:** 44953638871, `-f branch=main`, waybill `886792b7` (still `main`'s HEAD).

Each has two jobs: `Measure SBOM quality corpus` succeeded and `Report quality-corpus failure` was skipped. Both reports list 18 targets, with no violations and no config errors.

The `measurements[]` arrays were compared with `wall_ms` left out. Every field is equal (packages, edges, depth, graph completeness, sbomqs scores) except `sbom_bytes` on two targets:

| target | `main` | branch |
|---|---|---|
| cpp-mongo | 1,113,103 | 1,111,015 |
| python-pytorch | 605,529 | 599,961 |

These are the two quality-corpus targets whose census showed fetches or searches that change scope.

### Those two, component by component

The quality report keeps no documents. So `census-conditions.yml` (fixture repository) was dispatched twice: run 44959630495 at `49c12ee8` and run 44959632953 at `886792b7`. Each job's `refs.txt` names the waybill commit and the same target trees as `census.md`. Their CycloneDX documents were compared component by component, in document order, recursively including `metadata.component`.

| target | components (`main` / branch) | dependencies | changed components |
|---|---|---|---|
| cpp-mongo | 727 / 727 | identical | 9 |
| python-pytorch | 341 / 341 | identical | 24 |

Each document lists its root project twice, as `metadata.component` and as a component (`pkg:npm/mongodb@1.0.0`, `pkg:pypi/torch@0.0.0-unknown`). The same happens on `main`, so this spec did not introduce it.

**Every change has one shape:** Optional with C122 `cmake-conditional` becomes in scope, with no C122 and C232 or C236 kept. The fields that differ are `scope`, `waybill:lifecycle-scope` and `waybill:optional-derivation`, and nothing else.
- **cpp-mongo:** 3 fetches (`aws-sdk-cpp@1.11.17`, `azure-sdk-for-cpp@azure-storage-blobs_12.10.0`, `google-cloud-cpp@v2.36.0`), the census's 3. Plus 6 searches: `awssdk`, `azure-core-cpp`, `azure-storage-blobs-cpp`, `google_cloud_cpp_common`, `google_cloud_cpp_storage` and `sqlite3`. FR-001 and FR-004 for the fetches, FR-007 for the searches.
- **python-pytorch:** 1 fetch (`pkg:github/nvidia/nccl`), the census's one, which is unversioned. Plus 23 searches, within the census's upper bound of 29.

**A known over-report, as FR-004 chose.** Among pytorch's 23 searches are its mutually exclusive BLAS backends: `apl`, `atlas`, `blis`, `flexiblas`, `mkl`, `nvpl_blas`, `openblas` and `veclib`. Each search sits under `BLAS STREQUAL <name>`. That compares a cache string with no boolean default, which the evaluator reads as unknown (research R2), so each is kept in scope. A default build selects one of them. Reading `set(… CACHE STRING …)` defaults into `STREQUAL` is a follow-up (#1656), not this spec. `cuda` is a different case: it comes in because `USE_CUDA` defaults ON (`option-on`, holds).

## Runs at `8d76de26`

These ran before the two fixes in `therock.md` and `gate-log.md`:
- public corpus 44953634066: 67 of 67 identical;
- quality corpus 44953636382: equal to the final result above, field for field.
