# T022: corpora

## Public corpus (SC-002, SC-003)

Run 44600863894, branch `c183fe3e`, `regen_goldens=true`: 21 of 21 targets regenerated, and every `corpus_*` test passed. Comparing the artifact's goldens with the committed ones file by file, **6 files in 2 targets differ.** Both were candidates from the walk in spec Background. The nix-closure candidates did not change, as the Background warned about the walk.

| Target | Components | Scope changes | Traced to (non-top runtime main modules) |
|---|---|---|---|
| go-opentelemetry | 334 → 334 | 5, all `transitive` → required | `otel/sdk`, `sdk/log`, `sdk/metric`, `google/uuid` and `x/sys`, through modules such as `exporters/otlp/otlplog/otlploggrpc`, `otlploghttp`, `stdout/stdoutlog` and `bridge/opencensus` |
| haskell-security-advisories | 251 → 251 | 51, all `transitive` → required | `hsec-core`, `hsec-sync`, `hsec-tools`. These are the libraries of the three runtime packages: `vector`, `hashable`, `QuickCheck`, `scientific`, … |

**Nothing else changed in either document.** With scope, C42 and C62 removed from every component, the old and new documents are identical, and no metadata property changed. The goldens are committed with this record.

## Quality corpus

**CI.** Branch run 44600877896 against a fresh `main` control, run 44600890569: identical summary lines on all 18 targets.

**Documents.** The six locally cached targets were scanned with `main`'s binary (worktree at `1647ee01`) and the branch's (`c183fe3e`), using the harness's invocation and an empty `HOME`:

| Target | Components | Scope changes |
|---|---|---|
| python-ansible | 372 → 372 | 0 |
| cpp-mongo | 726 → 726 | 0 |
| cmake-nlohmann-json | 20 → 20 | 0 |
| python-flask | 108 → 108 | 0 (4 main modules, 51 `transitive`; none reached at runtime from a non-top project) |
| python-pytorch | 321 → 321 | 0 |
| pants-backend-ai | 340 → 340 | 0 |
