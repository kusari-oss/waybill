# T026 — the implemented walk, scored (SC-003)

Run 2026-11-10 on aarch64-darwin, Determinate Nix 3.20.0 (nix 2.34.6), nixpkgs `a799d3e3`. waybill built `--release` from branch `1423-us2` (parent `c4893669` plus the US2 working tree that this directory is committed with).

## What was run

1. `run.sh` scans the five wrapper flakes (curl, openssh, httpie, gh, ripgrep) with `gen-sboms.sh`'s flags, deps.dev on. A wrapper `nix` first on `PATH` answers every `path-info` the way nix answers for an output no store holds, and passes everything else to the real nix. All five outputs *are* in the local store (`../rt/<t>.paths` was read from it), so without the wrapper these scans take the runtime closure.
2. `score.py` scores three markings against one truth over one set of findings:
   - **truth:** `runtime.py`'s R(t), recomputed exactly as `runtime.py` does (NV + EMB; needs `rt/` and `rtscan/` under `NOISE_DIR`).
   - **reach:** `reach.py`'s candidate, recomputed exactly as `reach.py` does.
   - **walk:** the document waybill emitted under the wrapper. Runtime means not `scope: excluded`.

   Findings are the stored grype 0.120.0 matches on the original documents (`OUT_DIR/<t>/waybill.cdx.json.grype.json`), joined to the walk document by PURL.

```sh
WAYBILL=target/release/waybill TARGETS_DIR=… WALK_DIR=… bash run.sh
python3 -I -B score.py "$OUT_DIR" "$NOISE_DIR" "$WALK_DIR" > score.json
```

## Controls

- `../reach.py` and `../runtime.py` rerun on the same inputs reproduce the committed `reach.json` and `runtime.json`: both parse to values equal to the committed files.
- `selfcheck.json` is `score.py` with the *original* documents in place of the walk documents. It reproduces today's marks exactly as the parent README states them: 1,126 findings left as runtime, 350 true, 31.1% precision, 100% recall. So the scorer can tell a bad marking from a good one.
- Every walk document carries C216 `method: reachability-walk`, `unavailable-reason: not-in-any-store`; every closure member carries C203 `reachability-walk` except the evaluated package (`exempt-evaluated-package`).
- Reached derivations, from waybill's INFO line, against `reach.json`: curl 27/27, openssh 10/10, httpie 29/29, gh 1/1, ripgrep 2/2. Derivation totals agree too (746, 732, 2,458, 788, 897).
- 0 of 1,301 findings failed to join to the walk document by PURL.

## Result (finding level)

| target | findings | truth runtime | walk: runtime | TP | FP | FN | reach.py: TP / FP / FN |
|---|---|---|---|---|---|---|---|
| curl | 211 | 100 | 115 | 100 | 15 | 0 | 100 / 15 / 0 |
| openssh | 237 | 70 | 70 | 70 | 0 | 0 | 70 / 0 / 0 |
| httpie | 355 | 119 | 100 | 100 | 0 | 19 | 100 / 0 / 19 |
| gh | 266 | 55 | 55 | 55 | 0 | 0 | 55 / 0 / 0 |
| ripgrep | 232 | 6 | 6 | 6 | 0 | 0 | 6 / 0 / 0 |
| **total** | **1,301** | **350** | **346** | **331** | **15** | **19** | **331 / 15 / 19** |

- **Walk:** precision 331 / 346 = 95.66%, recall 331 / 350 = 94.57%.
- **reach.py:** the same, 95.66% and 94.57%.
- The two agree on every one of the 1,301 findings (`disagreements_with_reach` is empty for every target in `score.json`).

**SC-003** sets the committed figures as a floor. The implemented walk equals them exactly. It does not exceed them, and was not expected to: it is the same rule.

**Rounding.** SC-003 first stated the floor as ≥ 95.7% / ≥ 94.6%: these figures rounded *up*, so read literally the walk missed both (95.66% < 95.7%, 94.57% < 94.6%). It now states the exact fractions, precision ≥ 331/346 (95.66%) and recall ≥ 331/350 (94.57%). The measurement is unchanged; only its statement is.

## Where the walk is wrong

These are `reach.py`'s errors, inherited unchanged.

- **curl, 15 false runtime:** libxml2 (10 findings), c-ares (3) and libxslt (2). The walk reaches them through dependency fields of reached members, and curl's default output does not reference them (`../reach.json` `fp_sample`).
- **httpie, 19 missed:** libxml2 (10 findings), pcre2 (6) and coreutils (3). httpie's runtime closure references them, but no dependency field the walk follows from httpie reaches them (`../reach.json` `fn_list`). The walk's other httpie misses (apple-sdk, libSystem, libiconv and the like) carry no findings.

The static-link addition (FR-002 under the walk) found nothing on these five targets (`statically_linked=0` in every log), because none of them reaches a Haskell-builder member. It cannot add anything the walk misses when the source is `derivation-fields`: the fields it follows (`buildInputs`, `propagatedBuildInputs`) are a subset of `ARTIFACT_FIELDS`, which the walk already follows transitively.

## Not covered here

A static Haskell executable built with `justStaticExecutables` (shellcheck's shape, `../nix-behaviour/q6-*`) reaches its static build only through `inputs.drvs` and an `installPhase` path, never a dependency field. The walk therefore reaches nothing from such a root, and used as is it would mark every linked library. None of the five targets has this shape, so this score says nothing about it.

Since the US2 follow-up, such a walk is not used. When the walk reaches no member but the evaluated root **and** that root consumed a Haskell build through `inputs.drvs`, the role rule decides and C216 records `method: closure-role`, `walk-outcome: reached-only-root` (`contracts/c216-closure-scope-method.md`). The Haskell condition is what keeps this score unchanged: gh's walk also reaches only its root (`../reach.json`: 1 of 788 derivations), and on gh that walk is exact (55 true, 0 false above), while the role rule leaves 185 false-runtime findings there (`selfcheck.json`, `targets.gh.counts.walk_fp`). gh's root consumes no Haskell build, so it keeps the walk. Without the Haskell condition the guard would fire on gh and, by `selfcheck.json`, take precision from 331/346 to about 331/531 **(inference: that arm was not run)**.

**Re-run after the change** (`score-followup.json`, 2026-11-10, same host and inputs, `run.sh` + `score.py` with a debug build of branch `1423-us2-followups` before its commit): every target's C216 is `method: reachability-walk`, `walk-outcome: used`; every target's counts and the totals equal `score.json`'s (331 / 15 / 19); `disagreements_with_reach` is empty for every target.
