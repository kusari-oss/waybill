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
