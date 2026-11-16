# Corpus golden review (SC-002, SC-004)

**Method:** `public-corpus.yml` with `regen_goldens=true`, dispatched with `--ref 1537-requirements-root-attach -f branch=1537-requirements-root-attach`. Every regenerated file was compared with `cmp` against `waybill-cli/tests/fixtures/public_corpus/`. Contract I-1 was checked with `measurements/root_targets.py --dir` on every target's three documents.

**Validating `root_targets.py`:**
- On the T001 arms it reproduces `repro.md`: c2 differs (1 / 1 / 3), and c1 and c4 agree.
- The first corpus run exposed a flaw in it. Every file-tier PURL is `pkg:generic/file-tier?content-sha256=…`, so stripping qualifiers merged all of them into one CycloneDX target. Meanwhile SPDX file packages carry no PURL. File-tier entries are now keyed by content SHA-256 in all three formats: the qualifier, `checksums`, or `verifiedUsing`.

## Step 1: `root_edges` alone (T010)

Run 43935036519, scanned commit `6becc965` (US2 only), workflow definition from the same commit.

- **Goldens changed: 0 of 67** (63 goldens over 21 targets, 3 `recall-truth.json`, `README.md`). The workflow's summary and `cmp` agree.
- **I-1 on the corpus (SC-002):** 21 of 21 targets agree across CycloneDX, SPDX 2.3 and SPDX 3 on the root's direct targets.

**What this means for R9 cases 3–7.** No corpus target exercises any of them: an anchored root with no relationship edge, an SPDX non-synthetic root without edges, an SPDX 2.3 resolve-only root, or a nested graph-top.

The only place a case-5 change was observed is the `transitive_parity_pip_poetry` fixture. That is not a corpus target, and it is recorded in `tasks.md` T009: 64 → 89 SPDX 2.3 edges. That fixture's root targets went from 25 / 0 / 0 at `main` to 25 / 25 / 25.

Nothing to explain or commit for step 1.

## Step 2: declared attachment (T018)

**Run 43961229275,** which scanned `74ff20a7` (US1 plus the corrections in research R15), using the workflow from the same commit.
- 157 of 158 tests passed.
- The one failure, `m1218_dead_lockfile_entry_withheld_raises_the_residual_count`, is an ordering race peculiar to regen mode. The test reads `python-flask`'s golden from disk while the target tests are rewriting it, and it read the old file.
- Against the committed new goldens, it passes locally, with all five `m1218_*` tests and `golden_matrix_is_evaluable_and_gated`.

**An earlier run, 43939649463 at `502231ff`, was superseded.** Its flask, ansible and pytorch shapes were the two defects in R15.

**Compare mode as a cross-check.** Run 43957187975 at `e3dfd637` was dispatched without `regen_goldens` by mistake. It drifted on exactly the same five targets and on no other.

**Goldens changed: 15 of 67, all three formats of five targets.**

Every change was compared as `from-PURL -> to-PURL` edges, plus a key-sorted diff of everything outside `dependencies`. **Nothing was removed, and no component count or verdict changed.**

| target | edges added | what | R9 case |
|---|---|---|---|
| go-opentelemetry | 1 | `otel -> codespell`: the scan-root `requirements.txt` belongs to the scan-root Go module (FR-002, FR-003, clarification 4). codespell's `foreign-build-system` marker is gone, because it is now reached. Orphans: 265 → 264 | 1 |
| haskell-language-server | 4 | `haskell-language-server (hackage) -> docutils, myst-parser, sphinx, sphinx-rtd-theme`: `docs/requirements.txt`, no project of its own, walks up to the scan-root project. Typed optional, from `docs/`. Orphans: 6 → 2 | 1 |
| pants-example-django | 1 | `root -> mypy`: no project anywhere, so the subject owns root `requirements.txt` (FR-002). Already reachable through the resolve | 2 |
| pants-example-python | 2 | `root -> ansicolors, pytest`: as django | 2 |
| python-flask | 26 | edges between the pins of `examples/celery/requirements.txt`, from its `# via` lines, one for one (FR-002a, #1234). Orphans: 23 → 10, the ten explained `uv.lock` forks | 4 |

**On the regenerated documents:**
- **I-1 (`root_targets.py --dir`):** 21 of 21 targets agree.
- **I-3:** I7 passes for python-flask. Its declaration is removed.
- **I-2 / root-only:** see `root-only.md`.
