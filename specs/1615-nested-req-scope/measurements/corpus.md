# T019: corpora

## Public corpus (SC-005)

Run 44550878131, branch `fbb168ab`, `repeat=2`: **21 `corpus_*` target tests ok.** No golden changed. As research R7 predicted, no golden's pip source files have a new shape.

## Quality corpus (SC-006)

**CI counts.** The branch (run 44550893514, `fbb168ab`) against a fresh `main` control (run 44550911659, `52b96f1d`). All 18 targets have identical summary lines: components, edges, files, depth, completeness. A scope change moves none of these, and the runs upload only score reports, so the documents were compared locally.

**Documents**, compared locally:
- **Binaries:** `main`'s built from a worktree at `52b96f1d`; the branch's at `fbb168ab`.
- **Invocation:** the harness's exact one (`--offline sbom scan --path <checkout> --root-name <target> --root-version <pin>`), with an empty `HOME`, `GOMODCACHE` and `GOPATH`.
- **Inputs:** the cached checkouts at the pinned commits, for the six targets cached locally.

| Target | Components | Changed | What changed |
|---|---|---|---|
| python-ansible | 372 → 372 | 6 | required → test. 5 from `test/units/requirements.txt`. 1 declared in two `tests/{unit,integration}/requirements.txt` files nested in a test collection; parents `unit` and `integration`, nearest `tests` ancestor. |
| cpp-mongo | 726 → 726 | 1 | wiredtiger's vendored `test/3rdparty/testtools-2.7.1/requirements.txt`: already non-runtime, gains C155 `test`. FR-004's vendored-under-tests case. |
| cmake-nlohmann-json | 20 → 20 | 8 | required → docs, all from `docs/mkdocs/requirements.txt` |
| python-flask | 108 → 108 | 0 | |
| python-pytorch | 321 → 321 | 0 | |
| pants-backend-ai | 340 → 340 | 0 | |

**Exactly research R7's prediction:** the three movers, each through the predicted file. No component was added or removed, and none changed through spec 1269's propagation. The other 12 targets have no local cache; their CI counts are unchanged.
