# waybill vs sbomnix on Nix closures, for vulnerability management (2026-11-12)

The question: does waybill's `--nix-closure` output lead to better vulnerability findings than sbomnix's, for the scanners people actually run on them?

**The scanners compared:**
- **grype 0.120.0:** CPE matching, OpenVEX.
- **osv-scanner 2.7.0:** PURL ecosystems.
- **Trivy:** already measured on #1436. It matches nothing on `pkg:generic` Nix components from either tool, so it isn't compared here.

Everything here can be reproduced from this directory. `2026-11-12-nix-vs-sbomnix/run.sh` holds every command; the scripts and the result tables sit beside it.

## Verdict

On every target measured, waybill finds at least as many real vulnerabilities as sbomnix. It reports none of the CVEs nixpkgs has already patched, and it has no name-collision false positives.

Two findings across the five nixpkgs targets remain that only sbomnix reports. Neither is a package waybill misses (see below).

| | sbomnix 1.9.0 | waybill (main at `3a00c3e0`) |
|---|---|---|
| Real findings the other tool misses, Haskell targets | 0 | 42 (13 + 15 + 14) |
| Real findings the other tool misses, 5 nixpkgs targets, build time | 2, both disputed | 273 |
| Recall of the committed truth, Haskell targets | — | 100% (300/300, 268/268, 157/157) |
| Already-patched CVEs reported, 5 nixpkgs targets, build time | 96 | 0 (suppressed by VEX, #1436) |
| Name-collision false positives, 5 nixpkgs targets, build time | 25, plus 2 questionable | 0 |
| osv-scanner findings (distinct package and advisory) | 0 on every document | 8 / 10 / 13 on the Haskell targets; 39 / 66 / 31 on httpie / gh / ripgrep; 0 on curl and openssh |

## Setup

**Tool versions:**
- **sbomnix 1.9.0+g95480c2,** built from `github:tiiuae/sbomnix/95480c2…` (the v1.9.0 tag). nixpkgs' own sbomnix 1.9.0 (`0bb2eedc`) fails to build: `sqliteschema` 2.0.1 wants `typepy<2` and gets 2.0.0.
- **grype 0.120.0** from nixpkgs `0bb2eedc`, with database v6.1.10 built 2026-11-08 (sha256 `dcca2472…`), imported into an isolated cache. These are the same pins as `.github/workflows/recall-truth.yml`.
- **osv-scanner 2.7.0.**
- **waybill:** main at `3a00c3e0`. That includes:
  - #966: GHC boot-library versions;
  - #1478: boot-library scope;
  - #1436: backport VEX;
  - #1435: second curated CPE vendor visible to grype;
  - #1487: lower-case patch names;
  - #1489: the CPE rows below.

**Rules shared by both tools:**
- **What counts as one finding:** a distinct (canonical id, normalised package name) pair, the join key of `specs/1283-nix-allow-ifd/measurements/recall/classify.py`.
- **Haskell name collisions are excluded,** by that file's committed rule. A collision is a Haskell package matched to an advisory about another product.
- **Already patched** means the (CVE, package) appears in waybill's `pedigree.patches[].resolves`, which records the nixpkgs backports applied to that build. That record is derived from patch file names, so it undercounts a patch whose name carries no CVE id.
- **waybill's VEX** comes from each document's CycloneDX `vulnerabilities[]`. #1436 FR-006 makes it equal to the emitted sidecar, and `sidecar_and_cdx_agree` tests that.
- **waybill runtime view** means the document with components scoped `excluded` removed.

### Targets

- **Haskell closure corpus targets** (slack-web, openai, slacklinker).
  - sbomnix's side: its runtime CycloneDX and grype's findings on it, from `recall-truth.yml` run 43107760056 (x86_64-linux).
  - waybill's side: the committed Linux goldens.
  - The truth set is the committed `recall-truth.json`.
- **nixpkgs wrapper flakes** (curl, openssh, httpie, gh, ripgrep) over nixpkgs `a799d3e3`, on aarch64-darwin. These are the targets of the 2026-11-09 spike.
  - Both tools ran locally, in both modes.
  - The only flakes evaluated were nixpkgs and sbomnix's own (to build sbomnix).

## Haskell closure targets (runtime)

`haskell-results.tsv`:

| target | sbomnix grype | sbomnix already-patched | waybill grype + VEX | waybill osv | truth | waybill recall | sbomnix-only real | waybill-only real |
|---|---|---|---|---|---|---|---|---|
| slack-web | 294 | 2 | 297 | 8 | 300 | 300/300 | 0 | 13 |
| openai | 261 | 0 | 266 | 10 | 268 | 268/268 | 0 | 15 |
| slacklinker | 150 | 0 | 151 | 13 | 157 | 157/157 | 0 | 14 |

- **sbomnix's two already-patched findings** are binutils CVE-2025-5244 and CVE-2025-5245. grype finds them on waybill's document too (299 findings), and waybill's VEX suppresses them (297).
- **What waybill finds that sbomnix doesn't** falls into three groups:
  - **Haskell advisories, through osv-scanner:** `aeson`, `scientific`, `text-iso8601`, `crypton*`, `warp`, `cryptonite`, `base`, `Cabal`, `process`. sbomnix's `pkg:nix` PURLs can't match these. openai's `process` hit is HSEC-2024-0003, which affects Windows only.
  - **Rust crates read from binaries** (slacklinker).
  - **C libraries where sbomnix's CPE guess fails.** sbomnix repeats the name as the vendor (`coreutils:coreutils`, `xz:xz`, `brotli:brotli`, `ngtcp2:ngtcp2`), which matches nothing. waybill's curated rows (`gnu:coreutils`, `tukaani:xz`, `google:brotli`, `tatsuhiro-t:ngtcp2`) match NVD, for example CVE-2016-2781 on coreutils and CVE-2025-31115 on xz 5.6.3.

## nixpkgs wrapper targets

### Runtime (sbomnix default vs waybill's runtime view)

Raw grype matches: sbomnix | waybill | waybill with VEX (`runtime-matches.txt`).

| target | sbomnix | waybill | waybill + VEX |
|---|---|---|---|
| curl | 100 | 100 | 99 |
| openssh | 28 | 71 | 71 |
| httpie | 109 | 132 | 132 |
| gh | 0 | 55 | 55 |
| ripgrep | 6 | 6 | 6 |

- **The only finding only sbomnix has** is curl's one already-patched CVE, which waybill's VEX suppresses.
- **gh's 55** are Go modules read from the binary, which sbomnix doesn't see.
- **httpie's extra 23** are expat 2.8.0 CVEs (see below).

### Build time (sbomnix `--buildtime` vs waybill's whole document)

`wrapper-results.tsv`, right-product findings only:

| target | sbomnix | sbomnix collisions | waybill (grype + VEX, osv) | waybill collisions | sbomnix-only | of which already patched | remaining | waybill-only |
|---|---|---|---|---|---|---|---|---|
| curl | 218 | 0 | 226 | 0 | 19 | 19 | 0 | 27 |
| openssh | 246 | 0 | 254 | 0 | 19 | 19 | 0 | 27 |
| httpie | 316 | 25 (+2 questionable) | 365 | 0 | 22 | 20 | 2 | 71 |
| gh | 218 | 0 | 284 | 0 | 19 | 19 | 0 | 85 |
| ripgrep | 218 | 0 | 262 | 0 | 19 | 19 | 0 | 63 |
| **total** | **1,216** | **25** | **1,391** | **0** | **98** | **96** | **2** | **273** |

- **The 96 already patched** are 19 per target: unzip 13, jq 5 and libssh2 1, plus fontforge 1 on httpie. sbomnix reports them; waybill's VEX suppresses them in grype.
- **unzip:** grype finds 14 CVEs on `unzip 6.0` in both tools' documents, and waybill's VEX takes it to 1.
  - CVE-2022-0529 and CVE-2022-0530 are patched by `28-cve-2022-0529-and-cve-2022-0530.patch`. waybill reads that lower-case name since #1487.
  - The one left, CVE-2018-1000035, has no patch.
- **expat:** every target carries expat 2.8.0, and grype finds 23 CVEs on it in waybill's document and none in sbomnix's.
  - Both tools used to emit `libexpat:expat`, a pair that exists in NVD's dictionary but holds no CVEs.
  - NVD files expat's under `libexpat_project:libexpat`, which waybill emits since #1489.
- **httpie's 25 collisions** are Haskell packages in its build closure (`warp` ×13, `alex`, `async`, `cookie`, `foundation`, …). sbomnix's name-as-vendor CPE guesses tie them to unrelated products.

### The 2 findings only sbomnix has

| target | CVE | package | why waybill doesn't report it |
|---|---|---|---|
| httpie | CVE-2022-26345 | openmp | sbomnix matched `intel:openmp`, Intel's OpenMP runtime. nixpkgs' `openmp` is LLVM's, so this is a name collision the classifier doesn't catch. |
| httpie | CVE-2021-4048 | lapack | sbomnix matched `lapack_project:lapack` on nixpkgs' `lapack` provider wrapper, version `3`, through an open-ended range. The wrapper's version isn't LAPACK's. |

The first run of this comparison found 9 more. All were packages waybill named but had no CPE row for: ldns, rubygems, pixman, cairo, harfbuzz ×2, speex, libtiff and glib. #1489 added those rows, each probed against NVD's dictionary and title-checked, and waybill now reports all 9.

## Upstreaming the curated rows to nixpkgs

A CPE nixpkgs declares in `meta.identifiers.cpeParts` takes precedence over waybill's curated table (#1377). It also reaches every tool that reads nixpkgs' metadata, sbomnix included.

**Census of the 74 packages the curated table maps,** at nixos-unstable `d4f25a3e`:
- **25 already declare a CPE.** 23 agree with waybill, and glibc agrees on Linux. On libidn2, nixpkgs had it right and waybill's row was wrong (fixed in #1489).
- **49 declare none.** `nixpkgs-upstream-cpes.tsv` lists them with the vendor that NVD files their CVEs under, from `probe-cpe-dictionary.sh --cves`. The counts behind each choice are in `multivendor-cves.txt` and `single-vendor-cves.txt`.
- **Exceptions:** three packages need more than the one-line declaration. Their notes are in the TSV.
  - `pkgconf` is declared on `pkgconf-unwrapped`;
  - `compiler-rt` and `expat` need a `product` override.

## Limits

- **Platforms differ between the groups.** The Haskell targets were measured on Linux and the wrapper targets on aarch64-darwin, so their numbers are not comparable across the two groups.
- **The already-patched record is waybill's own reading of patch names.** Where it misses a patch, the CVE counts as real for both tools, which favours neither.
- **The two disputed findings are a judgement.** `classify.py` counts both as right-product. The reasoning for each is in the table above.
