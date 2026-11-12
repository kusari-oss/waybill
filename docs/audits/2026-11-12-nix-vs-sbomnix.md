# waybill vs sbomnix on Nix closures, for vulnerability management (2026-11-12)

The question: does waybill's `--nix-closure` output lead to better vulnerability findings than sbomnix's, for the scanners people actually run on them?

The consumers were grype 0.120.0 (CPE matching, OpenVEX), osv-scanner 2.7.0 (PURL ecosystems), and Trivy, already measured on #1436 (it matches nothing on `pkg:generic` Nix components from either tool).

Everything here can be reproduced from this directory. `2026-11-12-nix-vs-sbomnix/run.sh` holds every command; the scripts and the result tables sit beside it.

## Verdict

On every target measured, waybill finds at least as many real vulnerabilities as sbomnix, reports none of the CVEs nixpkgs has already patched, and has no name-collision false positives. sbomnix keeps one edge: 11 findings across the five nixpkgs targets, on packages waybill names but has no CPE row for (listed below).

| | sbomnix 1.9.0 | waybill (main + #1435) |
|---|---|---|
| Real findings the other tool misses, Haskell targets | 0 | 42 (13 + 15 + 14) |
| Real findings the other tool misses, 5 nixpkgs targets, build time | 11 | 158 |
| Recall of the committed truth, Haskell targets | — | 100% (300/300, 268/268, 157/157) |
| Already-patched CVEs reported, 5 nixpkgs targets, build time | 86 | 0 (suppressed by VEX, #1436) |
| Name-collision false positives, 5 nixpkgs targets, build time | 25, plus 2 questionable | 0 |
| osv-scanner findings (distinct package and advisory) | 0 on every document | 8 / 10 / 13 on the Haskell targets; 39 / 66 / 31 on httpie / gh / ripgrep; 0 on curl and openssh |

## Setup

**Tool versions:**
- sbomnix 1.9.0+g95480c2, built from `github:tiiuae/sbomnix/95480c2…` (the v1.9.0 tag). nixpkgs' own sbomnix 1.9.0 (`0bb2eedc`) fails to build: `sqliteschema` 2.0.1 wants `typepy<2` and gets 2.0.0.
- grype 0.120.0 from nixpkgs `0bb2eedc`, with database v6.1.10 built 2026-11-08 (sha256 `dcca2472…`), imported into an isolated cache. These are the same pins as `.github/workflows/recall-truth.yml`.
- osv-scanner 2.7.0.
- waybill: branch `1430-secondary-cpe-carrier` (#1435) merged with main at `89ed9e8a`. That is main with #966, #1478 and #1436, plus #1435's curated-CPE carrier.

**Rules shared by both arms:**
- Findings are distinct (canonical id, normalised package name) pairs, the join key of `specs/1283-nix-allow-ifd/measurements/recall/classify.py`.
- That file's committed rule excludes Haskell name collisions: a Haskell package matched to an advisory about another product.
- **Already patched** means the (CVE, package) appears in waybill's `pedigree.patches[].resolves`, which records the nixpkgs backports applied to that build. The oracle is filename-derived, so it undercounts: #1487 is a patch whose lower-case name it misses.
- **waybill's VEX** comes from each document's CycloneDX `vulnerabilities[]`. #1436 FR-006 makes it equal to the emitted sidecar, and `sidecar_and_cdx_agree` tests that.
- **waybill runtime view** means the document with components scoped `excluded` removed.

### Targets

- **Haskell closure corpus targets** (slack-web, openai, slacklinker).
  - sbomnix's side: its runtime CycloneDX and grype's findings on it, from `recall-truth.yml` run 43107760056 (x86_64-linux).
  - waybill's side: the committed Linux goldens.
  - The truth set is the committed `recall-truth.json`.
- **nixpkgs wrapper flakes** (curl, openssh, httpie, gh, ripgrep) over nixpkgs `a799d3e3`, aarch64-darwin, the 2026-11-09 spike's targets.
  - Both tools ran locally, in both modes.
  - Only nixpkgs was evaluated, plus sbomnix's own flake to build sbomnix.

## Haskell closure targets (runtime)

`haskell-results.tsv`:

| target | sbomnix grype | sbomnix already-patched | waybill grype + VEX | waybill osv | truth | waybill recall | sbomnix-only real | waybill-only real |
|---|---|---|---|---|---|---|---|---|
| slack-web | 294 | 2 | 297 | 8 | 300 | 300/300 | 0 | 13 |
| openai | 261 | 0 | 266 | 10 | 268 | 268/268 | 0 | 15 |
| slacklinker | 150 | 0 | 151 | 13 | 157 | 157/157 | 0 | 14 |

- **sbomnix's two already-patched findings** are binutils CVE-2025-5244 and CVE-2025-5245. grype finds them on waybill's document too (299 findings), and waybill's VEX suppresses them (297).
- **What waybill finds that sbomnix doesn't** falls into three groups:
  - Haskell advisories through osv-scanner (`aeson`, `scientific`, `text-iso8601`, `crypton*`, `warp`, `cryptonite`, `base`, `Cabal`, `process`), which sbomnix's `pkg:nix` PURLs cannot match. openai's `process` hit is HSEC-2024-0003, which affects Windows only.
  - Rust crates read from binaries (slacklinker).
  - C libraries where sbomnix's CPE guess repeats the name as vendor (`coreutils:coreutils`, `xz:xz`, `brotli:brotli`, `ngtcp2:ngtcp2`) and matches nothing. waybill's curated rows (`gnu:coreutils`, `tukaani:xz`, `google:brotli`, `tatsuhiro-t:ngtcp2`) match NVD: for example CVE-2016-2781 on coreutils, and CVE-2025-31115 on xz 5.6.3.

## nixpkgs wrapper targets

### Runtime (sbomnix default vs waybill's runtime view)

Raw grype matches: sbomnix | waybill | waybill with VEX (`runtime-matches.txt`).

| target | sbomnix | waybill | waybill + VEX |
|---|---|---|---|
| curl | 100 | 100 | 99 |
| openssh | 28 | 70 | 70 |
| httpie | 109 | 109 | 109 |
| gh | 0 | 55 | 55 |
| ripgrep | 6 | 6 | 6 |

The only finding sbomnix has that waybill doesn't is curl's one already-patched CVE, which waybill's VEX suppresses. gh's 55 are Go modules read from the binary, which sbomnix does not see.

### Build time (sbomnix `--buildtime` vs waybill's whole document)

`wrapper-results.tsv`, right-product findings only:

| target | sbomnix | sbomnix collisions | waybill (grype + VEX, osv) | sbomnix-only | of which already patched | real gap | waybill-only |
|---|---|---|---|---|---|---|---|
| curl | 218 | 0 | 205 | 17 | 17 | 0 | 4 |
| openssh | 246 | 0 | 232 | 18 | 17 | 1 | 4 |
| httpie | 316 | 25 (+2 questionable) | 336 | 28 | 18 | 10 | 48 |
| gh | 218 | 0 | 263 | 17 | 17 | 0 | 62 |
| ripgrep | 218 | 0 | 241 | 17 | 17 | 0 | 40 |
| **total** | **1,216** | **25** | **1,277** | **97** | **86** | **11** | **158** |

- **The 86 already patched** are 17 per target, made up of unzip 11, jq 5 and libssh2 1, plus fontforge 1 on httpie. sbomnix reports them. waybill's VEX suppresses them in grype.
- **unzip:** grype finds 14 CVEs on `unzip 6.0` in both tools' documents, and waybill's VEX takes it to 3. Of those 3, CVE-2022-0529 and CVE-2022-0530 are patched by `28-cve-2022-0529-and-cve-2022-0530.patch`, whose lower-case name waybill does not read yet (#1487). CVE-2018-1000035 has no patch.
- **httpie's 25 collisions** are Haskell packages in its build closure (`warp` ×13, `alex`, `async`, `cookie`, `foundation`, …) that sbomnix's name-as-vendor CPE guesses tie to unrelated products.

### The 11 findings only sbomnix has

Each is on a package waybill emits without a CPE:

| target | CVE | package |
|---|---|---|
| openssh | CVE-2026-10846 | ldns |
| httpie | CVE-2021-4048 | lapack |
| httpie | CVE-2022-26345 | openmp |
| httpie | CVE-2022-36073 | rubygems |
| httpie | CVE-2023-37769 | pixman |
| httpie | CVE-2025-50422 | cairo |
| httpie | CVE-2026-113679, CVE-2026-113680 | harfbuzz |
| httpie | CVE-2026-116741 | speex |
| httpie | CVE-2026-4775 | libtiff |
| httpie | CVE-2026-58016 | glib |

These are curated-table gaps. Each needs a row probed against the NVD dictionary (`scripts/probe-cpe-dictionary.sh`) before it is added.

## Limits

- **Platforms differ between the groups.** The Haskell targets were measured on Linux and the wrapper targets on aarch64-darwin, so their numbers are not comparable across the two groups.
- **The already-patched oracle is waybill's own patch reading.** Where it misses a patch (#1487), the CVE counts as real for both tools, which favours neither.
- **Trivy** reports nothing on either tool's `pkg:generic` Nix components (#1436 measurements), so it is not compared here.
