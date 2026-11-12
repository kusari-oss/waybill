# A curated CPE row's second vendor, as grype reads it (#1430)

A bounded fix, not a Spec Kit milestone. This file is the measurement record.
Method follows `specs/nix-curated-cpe-gaps/` (PR #1426).

**Date**: 2026-11-10 | **Host**: aarch64-darwin, Determinate Nix
**nixpkgs**: `a799d3e3886da994fa307f817a6bc705ae538eeb` (the 1377 wrapper flakes and the
`nix-closure-moat` corpus pin) | **waybill**: `main@f891b4fe` before (the 1377 comparison
documents; `be8924f7` and `739a9c13` above it touch tests and CI only), this branch after
(debug build) | **grype** 0.120.0 (syft v1.54.0 decoders), DB schema v6.1.10 **built
2026-11-08T06:19:57Z**, `GRYPE_DB_AUTO_UPDATE=false` for every run | **Trivy** 0.71.1 |
**osv-scanner** 2.7.0 (`measurements/results/grype-db-status.txt`, `tool-versions.txt`)

## Why

waybill's curated `unzip` row is `info-zip:unzip`, then `unzip_project:unzip`.
The first went to CycloneDX `component.cpe`; the second only to
`waybill:cpe-candidates`. grype found **0** findings on `unzip 6.0` from
waybill's document on all six 1377 targets, and **14** from sbomnix's. NVD
files all 14 under `unzip_project`.

## 1. Which carrier does grype read? (step 1, before any code)

**Source.** grype 0.120.0 decodes SBOMs with syft v1.54.0
(`go version -m` on the binary). Its CycloneDX decoder,
`syft/format/internal/cyclonedxutil/helpers/cpe.go:decodeCPEs`, reads
`component.cpe` and **every** property named `syft:cpe23`; nothing in that
package reads `evidence`. It also decodes `metadata.component` as a package
(`decoder.go:49`). The SPDX decoder,
`syft/format/common/spdxhelpers/to_syft_model.go:extractCPEs`, reads every
`cpe23Type` external reference.

**Measurement.** `measurements/carrier-probe.sh` takes the real waybill
CycloneDX document of each target (a `--nix-closure` scan containing
`pkg:generic/unzip@6.0`), writes hand-edited copies, and scans each with grype,
Trivy and osv-scanner. Each variant edits the unzip component only, except
`allsyft`. Result (`results/carrier-cdx.tsv`; every target identical in shape):

| variant | unzip carries | grype on unzip | grype lost | Trivy / osv change |
| --- | --- | --- | --- | --- |
| base | `cpe` = info-zip | **0** | — | — |
| swap (positive control) | `cpe` = unzip_project | 14 | 0 | none |
| (a) evidence | + `evidence.identity[]` `{field: cpe, concludedValue: unzip_project}` | **0** | 0 | none |
| (b) syft1 | + `syft:cpe23` = unzip_project | **14** | 0 | none |
| (b) syft2 | + `syft:cpe23` = a fixture CPE, then `syft:cpe23` = unzip_project | **14** | 0 | none |
| allsyft | every component with C19 gets `syft:cpe23` for candidates[1..] | 14 | 0 | none |

- `evidence.identity[]` is **not read**, though it is the CycloneDX-native
  multi-valued slot (and waybill already puts the primary there).
- `syft:cpe23` is read, and more than one value is honoured (syft2).
- (c) nothing else: the decoder reads no other CPE location.
- `allsyft` also gains `CVE-2019-6293` on `flex 2.6.4` via `westes:flex`
  (flex's second curated vendor) on all six targets, and nothing on the
  `pkg:pypi` components that also carry C19: grype's python matcher does not
  use CPEs by default (`using-cpes: false`, `cmd/grype/cli/options/match.go`).

**SPDX 2.3** (`measurements/spdx-probe.sh`, `results/carrier-spdx23.tsv`): a
one-package document with `unzip 6.0` and its CPEs as `cpe23Type` references.

| refs | grype on unzip | Trivy |
| --- | --- | --- |
| info-zip only (what waybill emitted) | 0 | 0 |
| info-zip, unzip_project | 14 | 0 |
| unzip_project, info-zip | 14 | 0 |
| unzip_project only (control) | 14 | 0 |

grype reads every reference, in either order. Trivy does no CPE matching and
reports nothing for a `pkg:generic` package either way.

## 2. The fix

`generate::cpe::secondary_cpes`: on a **`pkg:generic`** component, every CPE
after the primary with a release version. CycloneDX gets one `syft:cpe23`
property per CPE, on `components[]` and `metadata.component`; SPDX 2.3 gets one
further `SECURITY / cpe23Type` reference per CPE, with the primary's provenance
comment. SPDX 3 already emitted every fully-resolved CPE and is unchanged.
`waybill:cpe-candidates` (C19) is unchanged in all three.

- **Why `syft:cpe23`.** Principle V puts standards-native fields first; the
  native one, `evidence.identity[]`, measured 0. `syft:cpe23` is the
  convention syft writes for exactly this. It is not a `waybill:*` key, so it
  has no catalog row; A12 in `docs/reference/sbom-format-mapping.md` records it,
  and the A12 parity extractor now reads it.
- **Why only `pkg:generic`.** Its CPEs come only from `GENERIC_LIBRARY_CPES`,
  every pair probed against NVD. Other ecosystems' extra candidates are vendor
  guesses (`debian:<name>`, `python-<name>`). grype's apk matcher matches every
  CPE it is handed (`grype/matcher/apk/matcher.go:cpeResults`, not gated by
  `using-cpes`), so exposing those would change Alpine results, which nothing
  here measured. The six corpus goldens with non-generic C19 components
  (deb, golang, maven, npm, pypi) are unaffected.
- **Why a release version.** A wildcard version reads as every version. Same
  predicate SPDX 3 uses (`is_fully_resolved_cpe23`).

Components that gain the carrier on the six targets: `c-ares` (1), `file` (2),
`flex` (1), `jq` (1), `unzip` (1), plus `less` (1) on httpie and moat.

### Tests

- `generate::cpe::tests::i1430_*` (4): curated second vendors returned in table
  order; single-vendor row returns none; deb / apk / pypi heuristic candidates
  return none; a wildcard-version candidate is skipped.
- `generate::cyclonedx::builder::tests::i1430_*` (2) and
  `generate::spdx::packages::tests::i1430_*` (1): the emitted carriers.
- `tests/closure_secondary_cpe.rs` (6): a stub-`nix` closure with `unzip` and
  `zlib` (real names, synthetic `9.9.9`): unzip carries `syft:cpe23` =
  unzip_project and both SPDX formats carry both vendors; zlib carries none;
  C19 unchanged; A12's parity extractor sees the second vendor and the
  CycloneDX set is a subset of both SPDX sets; the CycloneDX document is
  schema-valid; a control asserts the closure was read. (The parity test was
  added after the red run, so it was never run red.)

Run red before the implementation (4 unit and 2 integration failures; the
negative-scope tests passed vacuously against the stub, as expected).
Mutation: removing the `pkg:generic` gate (reverted with `cp`, `cmp` confirmed)
fails `i1430_heuristic_candidates_are_not_secondary_cpes`,
`i1430_heuristic_candidates_get_no_syft_cpe23` and
`i1430_curated_secondary_cpes_are_further_cpe_refs`.

## 3. Scanner outcome

Before: the 1377 comparison documents, re-scanned now (every finding set
identical to the earlier scan). After: `gen-sboms.sh waybill` with this
branch's binary, same flags (deps.dev on). **One variable**: after removing
`syft:cpe23` properties, `serialNumber` and `metadata.timestamp`, every after
document is byte-identical (`jq -S`) to its before document.

grype, findings per target (`results/diffsets.txt`):

| target | before | after | lost | gained | gained elsewhere |
| --- | --- | --- | --- | --- | --- |
| curl | 211 | 226 | 0 | 15 | 0 |
| openssh | 237 | 252 | 0 | 15 | 0 |
| httpie | 355 | 370 | 0 | 15 | 0 |
| gh | 266 | 281 | 0 | 15 | 0 |
| ripgrep | 232 | 247 | 0 | 15 | 0 |
| moat | 220 | 235 | 0 | 15 | 0 |

- The 15 are the same on every target: the 14 sbomnix reports on
  `unzip 6.0`, and `CVE-2019-6293` on `flex 2.6.4`, which sbomnix does not
  report (its flex CPE is `flex_project`). "Gained elsewhere" counts gains on
  any component that did not gain a carrier.
- sbomnix-only grype findings fall 14 → 0 on curl, gh and ripgrep, 15 → 1 on
  openssh, 28 → 14 on moat, 130 → 116 on httpie (`results/*-analyze.txt`).
- Trivy and osv-scanner: identical before and after on every target. Trivy
  refreshed its own DB during the session (`tool-versions.txt`, downloaded
  2026-11-10T19:36Z); its finding sets were nonetheless identical across both
  arms and the earlier 1377 run, which is consistent with it doing no CPE
  matching at all.
- grype from the after **SPDX 2.3** documents equals grype from the after
  CycloneDX documents on all six targets (`vex-spdx-probe.sh`,
  `results/after-spdx23-vex.tsv`).
- Runtime-relevant grype findings are unchanged (295): unzip and flex are
  build-closure only.

## 4. Precision — read this before landing

`precision.py` (1377): a definite false positive is a finding whose CVE is
named by a patch on the component's own derivation.

| | grype findings, 6 targets summed | patched-by-nix |
| --- | --- | --- |
| before | 1,521 | 36 |
| after | 1,611 | **114** |

**13 of the 15 gained per target are patched by nixpkgs** (`results/patched-gained.txt`):
all of unzip's except `CVE-2018-1000035`. The patches are in the component's
CycloneDX `pedigree.patches[]`, which grype does not read. sbomnix's document
reports the same 13. The two not patched by name (`CVE-2018-1000035` on unzip,
`CVE-2019-6293` on flex) are an upper bound on true positives, not confirmed
ones.

The CPE is the right identifier either way; whether this build is affected is
a VEX question. But **waybill's VEX does not remove these for grype today**:
`vex-spdx-probe.sh` found the OpenVEX sidecar is not written at all for five of
the six targets (its "this build" subject is the `main-module` component, and
these wrapper-flake scans have none), and on moat, where it is written, the
patch-derived `not_affected` statements name the build as product with unzip
as a subcomponent, and `grype --vex` suppresses none of the 235.

## Not covered

- **Corpus golden.** The Linux `nix-closure-moat` golden has four `pkg:generic`
  components with C19 (c-ares, file, flex, unzip). It will gain 5 `syft:cpe23`
  properties and 5 SPDX 2.3 `cpe23Type` references. Corpus goldens are
  `#[ignore]`d locally and regenerated on Linux (`public-corpus.yml` with
  `regen_goldens`). No other corpus golden has a multi-CPE `pkg:generic`
  component.
- No non-corpus golden changed: the six golden-owning suites were run with
  `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1` and `git status` showed no
  fixture diff.
- Not measured: Alpine/apk, JVM and other CPE-matching paths — out of reach by
  construction, since only `pkg:generic` components gain a carrier.

## Reproduce

```sh
cargo build -p waybill --bin waybill
M=specs/1377-nix-subject-identity/measurements
N=specs/1430-secondary-cpe-carrier/measurements
W=<scratch dir>; T=<1377 wrapper-flake targets, as in specs/nix-curated-cpe-gaps/README.md>
export GRYPE_DB_AUTO_UPDATE=false GRYPE=<grype 0.120.0> TARGETS_DIR=$T

# step 1: carriers, on the before arm's documents
SRC_DIR=$W/before OUT_DIR=$W/probe bash $N/carrier-probe.sh
OUT_DIR=$W/probe bash $N/carrier-summary.sh > $N/results/carrier-cdx.tsv
OUT_DIR=$W/probe bash $N/spdx-probe.sh > $N/results/carrier-spdx23.tsv

# step 2: per arm (binary built at main, then at this branch)
OUT_DIR=$W/<arm> WAYBILL=<binary> bash $M/gen-sboms.sh waybill
OUT_DIR=$W/<arm> bash $M/scan.sh            # sbomnix documents copied in from the 1377 run
OUT_DIR=$W/<arm> DEST=$W/<arm>-summary bash $M/summarize.sh
OUT_DIR=$W/<arm> python3 -I $M/analyze.py; OUT_DIR=$W/<arm> python3 -I $M/precision.py
BEFORE=$W/before-summary AFTER=$W/after-summary bash $N/diffsets.sh
OUT_DIR=$W/after MEAS=$PWD/$M python3 -I $N/patched-gained.py
OUT_DIR=$W/vex WAYBILL=<after binary> bash $N/vex-spdx-probe.sh
```
