# T035 — the runtime-only recipe, scanned live (SC-001, SC-002)

Run 2026-11-11 on aarch64-darwin, Determinate Nix 3.20.0 (nix 2.34.6), nixpkgs `a799d3e3`. waybill 0.14.0 built `--release` from branch `1423-us4` at `48cefcc4`; the branch adds no production code before this run, only the T033 test and docs. grype 0.120.0, DB built 2026-11-08T06:19:57Z, the same DB as the committed truth, `GRYPE_DB_AUTO_UPDATE=false`.

## What was run

1. `run.sh` scans the five milestone-1377 wrapper flakes (curl, openssh, httpie, gh, ripgrep) with `gen-sboms.sh`'s flags, deps.dev on, in all three formats. The real nix answers: all five outputs are in the local store.
2. It applies the documented recipe, `waybill sbom edit <doc> --drop 'scope=excluded'`, to each format.
3. It runs grype on the full and the runtime-only CycloneDX documents.
4. `score.py` compares, finding by finding, keyed on (vulnerability id, PURL):
   - **truth:** the committed runtime truth. These are the grype matches on the original documents (`OUT_DIR/<t>/waybill.cdx.json.grype.json`, waybill `f891b4fe`) whose PURL is in `../runtime.py`'s R(t). R(t) is recomputed with `runtime.py`'s rule, so the 350 is re-derived, not copied.
   - **sbomnix runtime:** grype on sbomnix's runtime SBOMs (`OUT_DIR/<t>/sbomnix-rt.cdx.json.grype.json`). sbomnix and waybill name packages differently, so this join is on (id, normalised name, version), with `runtime.py`'s name normalisation.

```sh
WAYBILL=… GRYPE=… TARGETS_DIR=… US4_DIR=… bash run.sh
python3 -I -B score.py "$OUT_DIR" "$NOISE_DIR" "$US4_DIR" > score.json
```

`OUT_DIR` holds the 1377 documents and grype outputs; `NOISE_DIR` holds `rt/` and `rtscan/`, as for `../runtime.py`.

## Result

C216 (`waybill:closure-scope-method`) on every target: `method: runtime-closure`, `store: local`, `verified: true`, `unmatched-runtime-paths: 0`, `static-link-source: none`.

| target | grype findings, full: f891b4fe / this branch | runtime-only | truth | lost vs truth | added vs truth | sbomnix runtime | sbomnix findings missing |
|---|---|---|---|---|---|---|---|
| curl | 211 / 211 | 100 | 100 | 0 | 0 | 100 | 0 |
| openssh | 237 / 237 | 70 | 70 | 0 | 0 | 28 | 0 |
| httpie | 355 / 355 | 119 | 119 | 0 | 0 | 109 | 0 |
| gh | 266 / 266 | 55 | 55 | 0 | 0 | 0 | 0 |
| ripgrep | 232 / 232 | 6 | 6 | 0 | 0 | 6 | 0 |
| **total** | **1,301 / 1,301** | **350** | **350** | **0** | **0** | **243** | **0** |

- **SC-001 holds.** The runtime-only documents give 350 findings, the same 350 (id, PURL) pairs as the committed truth: none lost, none added.
- **SC-002 holds.** All 243 sbomnix-runtime findings are among them.
- **No drift.** The task expected drift, because the truth was built at `f891b4fe`. The full documents give the same 1,301 (id, PURL) pairs as the originals (`full_vs_original` is empty for every target). Between `f891b4fe` and `48cefcc4`, the only change to `waybill-cli/src/generate/cpe.rs` is one test-fixture field (`b6e1cefd`, `scope_evidence: None`). #1414 (subject CPEs) is an ancestor of `f891b4fe`, so the truth already includes it, and #1426 is not on this branch. The subject CPEs are unchanged on all five targets.

### The recipe itself

Per target, from `score.json` (`formats`, `edit_stderr`):

| target | PURLs dropped: CDX / SPDX 2.3 / SPDX 3 | same set in all three | not-in-artifact left | VEX statements removed (CDX) | VEX kept (CDX) |
|---|---|---|---|---|---|
| curl | 445 / 445 / 445 | yes | 0 | 34 | 2 |
| openssh | 445 / 445 / 445 | yes | 0 | 36 | 0 |
| httpie | 2,428 / 2,428 / 2,428 | yes | 0 | 46 | 0 |
| gh | 488 / 488 / 488 | yes | 0 | 36 | 0 |
| ripgrep | 940 / 940 / 940 | yes | 0 | 36 | 0 |

The subject is unchanged by the edit on every target (`subject`). Every edit exited 0.

### Components the recipe keeps that the truth doesn't name, and the reverse

These carry no findings, so they don't affect SC-001. Each is explained here because a component-level difference is still a difference (`components.kept_not_in_truth`, `components.truth_not_kept`).

- **`pkg:github/nixos/nixpkgs@a799d3e3…`, all five targets.** This is the flake input, not a closure member. It has lifecycle scope `build` (`waybill:lifecycle-scope`), so `scope=excluded` does not select it. `runtime.py` never counts it because it reads only closure components. The user guide states this.
- **httpie, `pkg:generic/libSystem@B`.** It is in httpie's runtime closure (`../rt/httpie.paths`, `y7k7…-libSystem-B`), and waybill keeps it with C203 `runtime-closure`. `runtime.py` misses it: its name-version split needs a version that starts with a digit, and `B` does not. This is a truth-side miss, so waybill is right here.
- **httpie, 26 versionless `pkg:hackage/*` boot libraries** (`base`, `Cabal`, `text`, …). They carry `waybill:haskell-version-unresolved-reason: compiler-supplied` and no `waybill:closure-role` or C203. They are not closure members, so no scope method marks them, and the recipe keeps them. They are build-only for httpie: they come from the GHC in its build closure. They carry no version, so grype matches nothing on them. **Inference, not run:** they come from the milestone-1276 compiler-supplied path. They are an over-inclusion in the runtime-only document, outside this feature's population.
- **httpie, 6 `pkg:generic/expand-response-params-<hash>` components the truth counts as runtime and the recipe drops.** The closure holds seven `expand-response-params` derivations with no `pname` and no version. The runtime closure references one of them, `g5cw2…-expand-response-params`, built by `zpgkhq0c…`. `runtime.py` matches on (name, version), which is (`expand-response-params`, none) for all seven, so it calls all seven runtime. waybill joins by output path (FR-003): it keeps `zpgkhq0c…` and marks the other six. This is a truth-side over-count, so waybill is right here.

## Files

| file | what it is |
|---|---|
| `run.sh` | scan, recipe, grype |
| `score.py` | the comparison |
| `score.json` | its output: per target C216, counts, the runtime-only (id, PURL) findings, the per-finding differences (all empty), the component-level differences above, and the edit's stderr |

The documents and raw grype output are not committed. They are large and `run.sh` regenerates them.
