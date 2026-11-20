# T021: corpora (SC-004)

## Public corpus

Run 44640074001, branch `e9c76676`, `regen_goldens=true`: 21 of 21 targets regenerated. Compared file by file with the committed goldens, **all 66 are byte-identical**. No public-corpus target has an installed-library record paired with a conditional or test-only search.

## Quality corpus

**CI.** Branch run 44640084108 against a fresh `main` control, run 44640094614: identical summary lines on all 18 targets.

**Documents.** The six locally cached targets (python-ansible, cpp-mongo, cmake-nlohmann-json, python-flask, python-pytorch, pants-backend-ai) were scanned with `main`'s binary (worktree at `857e542e`) and the branch's (`e9c76676`), using the harness's invocation and an empty `HOME`. Every document is **identical** once serial, timestamps and document namespace are removed.
