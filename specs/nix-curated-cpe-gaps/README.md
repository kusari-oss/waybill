# C libraries in Nix closures with no CPE

A bounded fix, not a Spec Kit milestone. This file is the measurement record.
It follows `specs/1395-nix-python-cpe/`, which did the same for CPython.

**Date**: 2026-11-10 | **Host**: aarch64-darwin, Determinate Nix
**nixpkgs**: `a799d3e3886da994fa307f817a6bc705ae538eeb` (the 1377 wrapper flakes and the
`nix-closure-moat` corpus pin) | **waybill**: `main@be8924f7` before, this branch after (debug builds)
**grype** 0.120.0, DB schema v6.1.10 **built 2026-11-08T06:19:57Z**, `GRYPE_DB_AUTO_UPDATE=false`
for every run (`measurements/results/grype-db-status.txt`) | **sbomnix** 1.9.0+g95480c2

## Why

In `--nix-closure` scans of the six 1377 targets, grype reports findings from
sbomnix's document that it does not report from waybill's. After #1395 the
largest remaining sources were components waybill emits with **no CPE**: nixpkgs
declares a product and no vendor for them (`meta.identifiers.cpeParts` has no
`vendor`, so `meta.identifiers.cpe` is null), and the curated table had no row.

sbomnix-only grype findings by component, before (`measurements/results/before-analyze.txt`,
grouped from `findings.json`):

| component | target | sbomnix-only | what it is |
| --- | --- | --- | --- |
| unbound 1.25.0 | httpie | 49 | NLnet Labs' DNS resolver, C |
| warp 3.4.9 | httpie | 13 | **Haskell** web server (Hackage) |
| libssh 0.12.0 | httpie | 11 | libssh, C |
| cmark-gfm 0.2.6 | moat | 10 | **Haskell** binding (Hackage) |
| libsndfile 1.2.2 | httpie | 6 | C |
| fontforge 20251009 | httpie | 4 | C |
| cjson 1.7.19 | httpie | 4 | C |
| giflib 5.2.2 | httpie | 3 | C |
| libass 0.17.4 | httpie | 2 | C |

## 1. Identity

`measurements/drv-dump.sh` (`nix derivation show -r` per target) and
`measurements/candidates.py` locate each derivation; `measurements/nixpkgs-meta.sh`
evaluates the attribute's `meta` at the pin. Source URLs from the derivations'
`structuredAttrs`:

| component | src | waybill PURL (before) | sbomnix CPE |
| --- | --- | --- | --- |
| unbound | `github.com/NLnetLabs/unbound/…release-1.25.0` | `pkg:generic/unbound@1.25.0` | `nlnetlabs:unbound` |
| libssh | `www.libssh.org/files/0.12/libssh-0.12.0.tar.xz` | `pkg:generic/libssh@0.12.0` | `libssh:libssh` |
| libsndfile | `github.com/libsndfile/libsndfile/…/1.2.2` | `pkg:generic/libsndfile@1.2.2` | `libsndfile_project:libsndfile` |
| fontforge | `github.com/fontforge/fontforge/…/20251009` | `pkg:generic/fontforge@20251009` | `fontforge:fontforge` |
| cjson | `github.com/DaveGamble/cJSON/…/v1.7.19` | `pkg:generic/cjson@1.7.19` | `davegamble:cjson` |
| giflib | `mirror://sourceforge/giflib/giflib-5.2.2.tar.gz` | `pkg:generic/giflib@5.2.2` | `giflib_project:giflib` |
| libass | `github.com/libass/libass/releases/…/0.17.4` | `pkg:generic/libass@0.17.4` | `libass_project:libass` |
| **warp** | **`mirror://hackage/warp-3.4.9.tar.gz`** | `pkg:hackage/warp@3.4.9` | `cloudflare:warp` |
| **cmark-gfm** | **`mirror://hackage/cmark-gfm-0.2.6.tar.gz`** | `pkg:hackage/cmark-gfm@0.2.6` | `github:cmark-gfm` |

All nine have `cpe: null` in nixpkgs. The seven C libraries are the upstream
projects. `warp` and `cmark-gfm` are Hackage packages: `haskellPackages.warp`
("A fast, light-weight web server for WAI applications") and
`haskellPackages.cmark-gfm` (homepage `hackage.haskell.org/package/cmark-gfm`).
nixpkgs' **C** cmark-gfm is a different attribute, `cmark-gfm-0.29.0.gfm.13`,
with the **same pname** `cmark-gfm`; it is in none of the six closures.

waybill gives the two Haskell packages `pkg:hackage` here because milestone
1275 confirmed them against the pinned package set. When that set is
unavailable the closure tier emits them as `pkg:generic/<pname>`, which is the
arm the curated table serves, so a `warp` or `cmark-gfm` row would reach them.

## 2. NVD

`measurements/nvd-probe.sh` makes one request per product with the vendor
wildcarded (`cpe:2.3:a:*:<product>`, `resultsPerPage=2000`, 7 s apart; every
product returned its whole dictionary in one page, which `nvd-summary.py`
checks). `nvd-summary.py` reduces it to `measurements/nvd-probe.out` and the
title file the table's enforcement test reads, `measurements/nvd-titles.txt`.
`scripts/probe-cpe-dictionary.sh --cves` then asked NVD how many CVEs cite
each pair (`measurements/nvd-cves.out`; controls `openssl:openssl` 330,
fixture pair 0).

| product | vendor : entries (deprecated) | title | exact nixpkgs version in dictionary | CVEs citing the pair |
| --- | --- | --- | --- | --- |
| unbound | **nlnetlabs** 206 (1) | NLnet Labs Unbound | 1.25.0 yes | 73 |
| | unbound_project 1 (1) | unbound Project unbound 1.6.6-5 | | 0 |
| libssh | **libssh** 53 (0) | Libssh | 0.12.0 no (dotted, same shape) | 45 |
| libsndfile | **libsndfile_project** 39 (0) | libsndfile Project libsndfile | 1.2.2 yes | 32 |
| | gentoo 2 (2) | Gentoo libsndfile | | not probed (all deprecated) |
| fontforge | **fontforge** 32 (0) | FontForge | 20251009 yes | 31 |
| | fontforge_project 25 (25) | FontForge Project FontForge | | 0 |
| cjson | **davegamble** 49 (0) | DaveGamble cJSON | 1.7.19 yes | 14 |
| | **cjson_project** 49 (48) | cJSON Project cJSON | | 3 |
| giflib | **giflib_project** 40 (0) | giflib project giflib | 5.2.2 yes | 14 |
| libass | **libass_project** 4 (0) | libass Project libass | 0.17.4 no (dotted, same shape) | 6 |
| warp | cloudflare 80 (0) | **Cloudflare WARP for Windows** | — | — |
| cmark-gfm | github 37 (0) | Github cmark-gfm **0.29.0.gfm.3** | 0.2.6 no; versions are `0.29.0.gfm.N` | — |

Version encoding: every chosen product files releases in the version field in
the form nixpkgs uses (dotted releases; fontforge's date `20251009`), with
pre-releases in the update field. nixpkgs' version is therefore usable
verbatim. libssh 0.12.0 and libass 0.17.4 are newer than NVD's newest
dictionary entry; a CPE matcher orders versions against advisory ranges, not
against dictionary entries, and grype matched both (section 4).

## 3. The fix

Seven exact-name rows in `GENERIC_LIBRARY_CPES` (`waybill-cli/src/generate/cpe.rs`):

| name | (vendor, product) |
| --- | --- |
| `cjson` | `davegamble:cjson`, then `cjson_project:cjson` |
| `fontforge` | `fontforge:fontforge` |
| `giflib` | `giflib_project:giflib` |
| `libass` | `libass_project:libass` |
| `libsndfile` | `libsndfile_project:libsndfile` |
| `libssh` | `libssh:libssh` |
| `unbound` | `nlnetlabs:unbound` |

plus their `NVD_DICTIONARY_OBSERVED` entries and the title file, which the
table's enforcement tests require. cjson emits both vendors because 3 CVEs
still cite the deprecated `cjson_project` namespace; `davegamble` is primary.

**Rejected**, recorded beside the table's other rejections:

- **`warp`.** NVD's only `warp` is Cloudflare WARP. The 13 findings sbomnix
  produces are Cloudflare WARP advisories (`found.cpes` carry
  `target_sw` = `windows`, `macos`, `android`) attached to a Haskell web
  server. All 13 are false positives.
- **`cmark-gfm`.** NVD's `github:cmark-gfm` is the C library, versioned
  `0.29.0.gfm.N`. The binding's `0.2.6` orders below every fixed version, so
  sbomnix's 10 findings come from a version in another project's numbering.
  The row would also be keyed on a pname nixpkgs shares between the binding
  and the C library, so it cannot tell them apart. Whether the C sources the
  binding bundles are affected by any of the 10 is a separate question this
  record does not answer.
- Also not emitted: `unbound_project`, `fontforge_project`, `gentoo:libsndfile`
  (deprecated namespaces; the first two have 0 CVEs).

### Tests

- `generate::cpe::tests::nix_gap_closure_c_libraries_map_to_nvds_pair` — each row, exact output.
- `generate::cpe::tests::nix_gap_bindings_and_prefixed_names_get_no_cpe` — `warp`
  and `cmark-gfm` (generic and hackage, and the C cmark-gfm version), hackage
  names, `python3.13-unbound`, `libssh-<x>`, `unbound-full`, `giflib-tools`,
  `libsndfile1`, a `cjson` tarball name, `fontforge-gtk`, `libass2`: no CPE.
- `closure_component_cpes::nix_gap_closure_c_libraries_carry_nvds_cpe` — a
  stub-`nix` closure (real names, synthetic version `9.9.9`): unbound, libssh
  and cjson carry the CPE in all three formats, recorded as curated, PURL
  unchanged; cjson's second vendor is in `waybill:cpe-candidates`.
- `closure_component_cpes::nix_gap_haskell_packages_and_bindings_get_no_cpe` —
  in the same closure, a Hackage-fetched `warp`, a `cmark-gfm` and
  `python3.13-unbound` are emitted and carry no CPE.

Mutation check (`cp` to revert; `cmp` confirmed the restore):

| mutation | failing tests |
| --- | --- |
| add a `warp` → `cloudflare:warp` row | `nix_gap_bindings…`, `every_table_pair_exists_in_the_nvd_dictionary`, integration `nix_gap_haskell…` |
| add a `cmark-gfm` → `github:cmark-gfm` row | same three |
| lookup by prefix (`name.starts_with(slug)`) | `nix_gap_bindings…`, `i1395_names_that_are_not_the_interpreter_get_no_cpe`, integration `nix_gap_haskell…` |
| drop the `unbound` row | `nix_gap_closure_c_libraries_map…`, `nix_gap_bindings…` (control), `no_dictionary_observation_is_stale`, both integration tests |

The `warp` mutation failing the integration test is also the evidence that the
fixture's `warp` is emitted as `pkg:generic`, i.e. that the rejection is live.

## 4. Scanner outcome

Probes from `specs/1377-nix-subject-identity/measurements/` unchanged:
`gen-sboms.sh waybill` (deps.dev on), `gen-sboms.sh sbomnix`, `scan.sh`,
`summarize.sh`, `analyze.py`, `precision.py`; this directory adds
`diffsets.sh` and `patched-gained.py`. In the before arm 50 of 51 scanner runs
exit 0/1, and in the after arm 35 of 36 (see below for the 15 not re-run); the
exception is osv on sbomnix's empty openssh runtime document, as in 1377 and
1395.

The sbomnix documents were generated once, in the *before* arm, and copied to
the *after* arm. For curl, openssh and httpie the after arm re-scanned them;
all 18 (target, document, scanner) result sets were identical to the before
arm's. For gh, ripgrep and moat the before arm's scan results were copied.

grype, findings per target (`results/diffsets.txt`, `results/*-analyze.txt`):

| target | before | after | lost | gained | sbomnix-only before → after |
| --- | --- | --- | --- | --- | --- |
| curl | 211 | 211 | 0 | 0 | 14 → 14 |
| openssh | 237 | 237 | 0 | 0 | 15 → 15 |
| **httpie** | 355 | **434** | 0 | **79** | 130 → **51** |
| gh | 266 | 266 | 0 | 0 | 14 → 14 |
| ripgrep | 232 | 232 | 0 | 0 | 14 → 14 |
| moat | 220 | 220 | 0 | 0 | 28 → 28 |

- The seven libraries are only in httpie's closure on this host, so only httpie
  moves. The 79 are unbound 49, libssh 11, libsndfile 6, cjson 4, fontforge 4,
  giflib 3, libass 2: exactly the sbomnix-only counts in section "Why".
  `shared` rises 213 → 292 (+79) and `waybill-only` is unchanged at 142, so they
  are the same (CVE, component, version) triples grype reports from sbomnix.
  Nothing was gained on any other component (`gained_elsewhere=0` everywhere).
- What remains sbomnix-only on httpie after this: unzip 14, warp 13, and 24
  findings across 19 other components (one or two each); on moat: unzip 14, cmark-gfm 10, parallel 2,
  async 1, alex 1 (see "Not covered").
- Trivy and osv-scanner: identical before and after on every target (they read
  PURLs; no PURL changed).
- **Runtime relevance.** None of the seven is in Nix's runtime closure of
  httpie: runtime-relevant grype findings stay 119 (sbomnix: 111). All 79 are
  build-closure findings.
- All 79 are `cpe-match`. 5 Critical, 31 High, 33 Medium, 10 Low
  (`results/httpie-gained-grype.tsv`). unbound's 49 all carry a fix version
  above 1.25.0 (1.25.1–1.26.2). 25 have no fix state (cjson 4, fontforge 4,
  giflib 3, libsndfile 6, libssh 8 — 6 of libssh's with no version constraint
  at all, i.e. NVD's configuration names the product without a version range).

## 5. Precision

`precision.py`: a definite false positive is a finding whose CVE is named by a
patch on the component's own derivation.

| | grype findings, 6 targets summed | patched-by-nix |
| --- | --- | --- |
| before | 1,521 | 36 |
| after | 1,600 | **37** |

**1 of the 79 is patched by nixpkgs** (`results/patched-gained.txt`):
fontforge 20251009, CVE-2025-15279. sbomnix's document reports the same
finding. The other 78 are not patched by name, which makes them an upper bound
on true positives, not confirmed ones. In particular the 6 libssh findings with
no version constraint match any libssh version, and none of the 79 was checked
for platform applicability.

## Not covered

- **unzip, 14 findings on every target.** waybill's existing `unzip` row emits
  `info-zip:unzip` as `component.cpe` and `unzip_project:unzip` only in
  `waybill:cpe-candidates`. grype reads `component.cpe` alone, and every one of
  the 14 is filed under `unzip_project`, so grype sees none of them from
  waybill's document. Not changed here: it is an existing row's ordering (and a
  broader question about secondary candidates being invisible to grype), not a
  missing row.
- The smaller sbomnix-only names (`zlib 0.7.1.1`, `parallel 3.2.2.0`,
  `libyaml 0.1.4`, `commonmark 0.2.6.1`, `harfbuzz 13.2.1`, `pixman`, `cairo`,
  `glib`, `libtiff`, `speex`, …) were not investigated. Several have
  Hackage-shaped versions and are likely the same class of sbomnix false
  positive as `zlib 0.7.1.1`.
- **Corpus golden.** The Linux `nix-closure-moat` golden contains
  `pkg:generic/unbound@1.25.0` with no CPE; it will gain
  `cpe:2.3:a:nlnetlabs:unbound:1.25.0:…` and its curated provenance, and C209's
  counts move. Corpus goldens are `#[ignore]`d locally and are regenerated on
  Linux (`public-corpus.yml` with `regen_goldens`). No other corpus target
  contains any of the seven names as `pkg:generic`. That golden's `cmark-gfm`
  is `pkg:hackage` and is unaffected.
- No non-corpus golden changed: the six golden-owning suites (`cdx_regression`,
  `spdx_regression`, `spdx3_regression`, `oci_pull_backward_compat`,
  `optional_dep_classification`, `pkg_alias_binding_us1`) were run with
  `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1` and `git status` showed no
  fixture diff.

## Reproduce

```sh
cargo build -p waybill --bin waybill
M=specs/1377-nix-subject-identity/measurements
N=specs/nix-curated-cpe-gaps/measurements
W=<scratch dir>
cp -R $M/wrapper-flakes/* $W/targets/
cp -R ~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo $W/targets/moat
for t in curl openssh httpie gh ripgrep; do (cd $W/targets/$t && git init -q && git add . && git -c user.name=q -c user.email=q@q commit -qm q); done
export GRYPE_DB_AUTO_UPDATE=false GRYPE=<grype 0.120.0> TARGETS_DIR=$W/targets

# identity
DRV_DIR=$W/drv bash $N/drv-dump.sh
DRV_DIR=$W/drv python3 -I $N/candidates.py unbound warp libssh cmark-gfm libsndfile fontforge cjson giflib libass
bash $N/nixpkgs-meta.sh unbound libssh libsndfile fontforge cjson giflib libass haskellPackages.warp haskellPackages.cmark-gfm cmark-gfm

# NVD
OUT_DIR=$W/nvd bash $N/nvd-probe.sh unbound libssh libsndfile fontforge cjson giflib libass warp cmark-gfm
OUT_DIR=$W/nvd python3 -I $N/nvd-summary.py unbound=1.25.0 libssh=0.12.0 libsndfile=1.2.2 \
  fontforge=20251009 cjson=1.7.19 giflib=5.2.2 libass=0.17.4 warp=3.4.9 cmark-gfm=0.2.6
bash scripts/probe-cpe-dictionary.sh --cves nlnetlabs:unbound unbound_project:unbound libssh:libssh \
  libsndfile_project:libsndfile fontforge:fontforge fontforge_project:fontforge davegamble:cjson \
  cjson_project:cjson giflib_project:giflib libass_project:libass

# per arm (binary built at main, then at this branch):
OUT_DIR=$W/<arm> WAYBILL=<binary> bash $M/gen-sboms.sh waybill
OUT_DIR=$W/before SBOMNIX=<sbomnix 1.9.0> bash $M/gen-sboms.sh sbomnix   # then copy $W/before/*/sbomnix-* into $W/after/*/
OUT_DIR=$W/<arm> bash $M/scan.sh
OUT_DIR=$W/<arm> DEST=$W/<arm>-summary bash $M/summarize.sh
OUT_DIR=$W/<arm> python3 -I $M/analyze.py; OUT_DIR=$W/<arm> python3 -I $M/precision.py
BEFORE=$W/before-summary AFTER=$W/after-summary bash $N/diffsets.sh
OUT_DIR=$W/after MEAS=$PWD/$M python3 -I $N/patched-gained.py
```
