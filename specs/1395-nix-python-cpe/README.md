# #1395 — CPython in a Nix closure carries no CPE

A bounded fix, not a Spec Kit milestone. This file is the measurement record.

**Date**: 2026-11-09/10 | **Host**: aarch64-darwin, Determinate Nix
**nixpkgs**: `a799d3e3886da994fa307f817a6bc705ae538eeb` (the 1377 wrapper flakes and the
`nix-closure-moat` corpus pin) | **waybill**: `main@9a3416d1` before, this branch after (debug builds)
**grype** 0.120.0, DB schema v6.1.10 **built 2026-11-08T06:19:57Z**, `GRYPE_DB_AUTO_UPDATE=false`
for every run (`measurements/results/grype-db-status.txt`) — the same DB as milestone 1377's results.

## The fix

Two rows in the curated table (`waybill-cli/src/generate/cpe.rs`,
`GENERIC_LIBRARY_CPES`), keyed on exact component names:

| name | (vendor, product) |
| --- | --- |
| `python3` | `python:python` |
| `python3-minimal` | `python:python` |

plus the dictionary observation (`NVD_DICTIONARY_OBSERVED`, `python:python` = 651)
and the title query (`measurements/nvd-titles.txt`) the table's enforcement
tests require. Since milestone 1377 a curated CPE on a closure component is
recorded as curated (`waybill curated CPE table` in CycloneDX evidence) without
further code.

## 1. Which CPE NVD uses for CPython

`measurements/nvd-probe.sh` (three requests, output in `measurements/nvd-probe.out`):

| query | totalResults |
| --- | --- |
| `cpe:2.3:a:python:python:3.13` (exact version) | **0** |
| `cpe:2.3:a:python:python` | **651** (89 deprecated) |
| `cpe:2.3:a:*:python` | 1000: `python` 651, `microsoft` 349 |

- Vendor `python`, product `python` ("Python Python …").
- The version field carries the full release: `3.13.13`, `3.12.13`, `3.14.4` each
  have an entry. Pre-releases go in the update field (`alpha1`…, `beta1`…,
  `rc1`…). A bare minor (`3.13`) is not how 3.13 releases are filed, which is
  why the first query is 0. nixpkgs' version string (`3.13.13`) is therefore
  usable verbatim.
- `microsoft:python` is "Microsoft Python … for Visual Studio Code", an editor
  extension. Not emitted.

## 2. What the interpreter is called in a closure

`measurements/interp-names.sh` (`results/interp-names.txt`), `nix eval` at the pin:

| attribute | name | pname | `meta.identifiers.cpeParts` |
| --- | --- | --- | --- |
| python3, python313, python313FreeThreading | `python3-3.13.13` | `python3` | product `python3`, no vendor |
| python312 | `python3-3.12.13` | `python3` | same |
| python314 | `python3-3.14.4` | `python3` | same |
| python3Minimal | `python3-minimal-3.13.13` | `python3-minimal` | product `python3-minimal`, no vendor |
| pypy3 | `pypy3.11-7.3.20` | `pypy3.11` | not CPython — no row |

`nix derivation show -r` on all six targets, then `measurements/pynames.py`
(`results/pynames.txt`): every closure holds `python3-3.13.13` and, beside it,
`python3-3.13.13-env` (a `withPackages` environment; no pname),
`python3.13-<pkg>` packages and `python-<hook>` hooks. `waybill debug
closure-dump` on httpie shows the interpreter emitted as `python3` (pname from
`structuredAttrs`) and the environment as `python3-3.13.13-env` with no version.
No darwin closure here contains `python3-minimal`; the committed **Linux**
`nix-closure-moat` golden does (`pkg:generic/python3-minimal@3.13.13`, no CPE),
which is why that row is included.

Every other Python-prefixed name is something else, so the rows are exact
names, never a prefix. `i1395_names_that_are_not_the_interpreter_get_no_cpe`
and `i1395_only_the_interpreter_gets_the_cpython_cpe` enforce it; both fail
when the lookup is mutated to a prefix match.

## 3. Scanner outcome

Probes from `specs/1377-nix-subject-identity/measurements/` unchanged:
`gen-sboms.sh waybill` (deps.dev on), `gen-sboms.sh sbomnix` (1.9.0+g95480c2),
`scan.sh`, `summarize.sh`, `analyze.py`, `precision.py`. 50 of 51 scanner runs
exit 0/1 in each arm; the exception is osv on sbomnix's empty openssh runtime
document, as in 1377. The *before* arm reproduces 1377's `results-final`
exactly (1,681 summary rows; grype 1,232 / patched 36 / runtime 260).

grype, findings per target (`results/diffsets.txt`, `results/*-analyze.txt`):

| target | before | after | lost | gained (all on `python3 3.13.13`) | sbomnix-only before → after |
| --- | --- | --- | --- | --- | --- |
| curl | 176 | 211 | 0 | 35 | 49 → 14 |
| openssh | 202 | 237 | 0 | 35 | 50 → 15 |
| **httpie** | 308 | 343 | 0 | 35 | 165 → 130 |
| gh | 176 | 211 | 0 | 35 | 49 → 14 |
| ripgrep | 185 | 220 | 0 | 35 | 49 → 14 |
| moat | 185 | 220 | 0 | 35 | 63 → 28 |

- Trivy and osv-scanner: identical before and after on every target (they read
  PURLs; the PURL did not change).
- The 35 are the same (CVE, component, version) triples grype reports from
  sbomnix's document: `shared` rises by exactly 35 and `waybill-only` is
  unchanged on every target.
- **Runtime relevance.** python3 is in Nix's runtime closure of httpie only.
  httpie runtime-relevant grype findings: 84 → **119** (sbomnix: 111). On the
  other five targets python3 is build-time only, so their 35 are build-closure
  findings.
- All 35 are `cpe-match`, all have a fix version (`fix.state = fixed`); 2
  Critical, 11 High, 17 Medium, 5 Low (`results/httpie-python3-grype.tsv`).

## 4. Precision

`precision.py` classifies a finding as a definite false positive when the
component's derivation carries a patch whose name or content names the CVE.

| | grype findings | patched-by-nix |
| --- | --- | --- |
| before, 6 targets summed | 1,232 | 36 |
| after, 6 targets summed | 1,442 | **36** |

**0 of the 35 new findings are patched by nixpkgs.** The python3 derivation
carries three patches — `no-ldconfig.patch`, `virtualenv-permissions.patch`,
`mimetypes.patch` — none naming a CVE. This is the only adjudication done: a
finding not patched by name is an upper bound on true positives, not a
confirmed one. In particular, CPE matching does not filter by platform, so a
Windows-only CPython advisory would still be reported against a darwin or Linux
build; that has not been checked per CVE.

## Not covered

- perl, ruby, nodejs interpreters in closures (the issue's open question). Not
  measured here. `perl` and `node` already have curated rows; whether they
  match nixpkgs' pnames was not checked.
- The `nix-closure-moat` corpus golden will change (`python3` and
  `python3-minimal` gain a CPE and its provenance; C209's counts move). Corpus
  goldens are `#[ignore]`d locally and are regenerated on Linux.
- No non-corpus golden changed: all six golden suites were run with
  `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1` and `git status` showed no fixture
  diff; no committed fixture outside the moat corpus contains
  `pkg:generic/python3` or `pkg:generic/python3-minimal`.

## Reproduce

```sh
cargo build -p waybill --bin waybill
M=specs/1377-nix-subject-identity/measurements
N=specs/1395-nix-python-cpe/measurements
W=$(mktemp -d)
cp -R $M/wrapper-flakes/* $W/       # outside any git repo: evaluated as path flakes
cp -R ~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo $W/moat
export GRYPE_DB_AUTO_UPDATE=false GRYPE=<grype 0.120.0> TARGETS_DIR=$W
OUT_DIR=$W/nvd bash $N/nvd-probe.sh
bash $N/interp-names.sh
mkdir $W/drv; for t in curl openssh httpie gh ripgrep moat; do nix derivation show -r "$W/$t#default" > $W/drv/$t.json; done
DRV_DIR=$W/drv python3 -I $N/pynames.py curl openssh httpie gh ripgrep moat
# per arm (binary built at main, then at this branch):
OUT_DIR=$W/<arm> WAYBILL=<binary> bash $M/gen-sboms.sh waybill
OUT_DIR=$W/<arm> SBOMNIX=<sbomnix 1.9.0> bash $M/gen-sboms.sh sbomnix
OUT_DIR=$W/<arm> bash $M/scan.sh
OUT_DIR=$W/<arm> DEST=$W/<arm>-summary bash $M/summarize.sh
OUT_DIR=$W/<arm> python3 -I $M/analyze.py; OUT_DIR=$W/<arm> python3 -I $M/precision.py
BEFORE=$W/before-summary AFTER=$W/after-summary bash $N/diffsets.sh
```
