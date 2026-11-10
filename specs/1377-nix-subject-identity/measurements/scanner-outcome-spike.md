# Spike: scanner outcomes on waybill vs sbomnix Nix SBOMs — 2026-11-09

**Question.** For vulnerability-management users (Kusari's OSV+CPE tools, grype,
Trivy), does a scanner produce more correct findings from waybill's Nix SBOM or
from sbomnix's, on the same derivation — and where does waybill lose?

**Setup (all measured, nothing assumed).**
- nixpkgs `a799d3e3886d` (2026-06-06), aarch64-darwin, Determinate Nix 3.20.0.
- Targets: wrapper flakes exposing `nixpkgs#{curl,openssh,httpie,gh,ripgrep}` as
  `.#default`, plus a copy of the `moat` corpus pin (flake.lock md5-verified).
- sbomnix 1.9.0+g95480c2: `--buildtime` (head-to-head) and runtime (reference).
- waybill `main@72b455e2`, debug build: `sbom scan --nix-closure --no-deep-hash
  --no-clearly-defined`, deps.dev confirmation ON.
- Scanners: grype 0.120.0 (DB built 2026-11-08), Trivy 0.71.1, osv-scanner 2.7.0.
  Kusari's matcher is proxied by osv-scanner (OSV-by-PURL) + grype's CPE path.
- Probes: `gen-sboms.sh`, `scan.sh`, `analyze.py`, `adjudicate.py`, `precision.py`.
  51 scans, all exit 0/1 except osv on sbomnix's *empty* openssh runtime SBOM.

## Results

### 1. PURL-based scanners see nothing in sbomnix's SBOM
Trivy and osv-scanner report **0 findings on every sbomnix SBOM** (`pkg:nix` is no
ecosystem either indexes; neither does CPE matching). On waybill's: httpie 128/135,
moat 13/17, ripgrep 13/14, curl/gh 4/4, openssh 3/3 (Trivy/OSV) — via `pkg:pypi`
and `pkg:hackage`. **For the OSV side of Kusari's matching, waybill > 0.**

### 2. CPE path (grype): sbomnix finds more, waybill is more precise
| grype, 6 targets summed | findings | confirmed FP (CVE Nix already patches) | on shipped components |
|---|---|---|---|
| sbomnix | 1,475 | **115 (8%)** | 288 findings, 1 FP |
| waybill | 1,203 | **36 (3%)** | 232 findings, 1 FP |

"Confirmed FP" = the component's own derivation carries a patch whose name or
content names that CVE (positive control: unzip's 13 CVE-named patches, all
detected). Detection only catches CVE-*named* patches, so the remainder is an
upper bound on true positives, not a count of them.

**Why sbomnix finds more:** of 205 distinct grype findings only sbomnix surfaces,
**191 are components waybill has but gives no CPE**; 14 have a CPE grype didn't
match; **0 are components waybill lacks**. 14 of the 205 are confirmed FPs.
Runtime-relevant misses: **python3 3.13.13 (35 CVEs)** and **openssh 10.3p1, the
scanned target itself (28 CVEs)**.

**Where sbomnix's CPEs come from:** with `--exclude-cpe-matching` it keeps 17 of
662 on openssh. **97% are heuristic** — including `filename:filename::*` CPEs on
every tarball and patch, and `python3.13-httpie:python3.13-httpie` for httpie.
nixpkgs `meta.identifiers.cpe` is authoritative where set (openssh, curl,
openssl, zlib; not python313/httpie, which lack a vendor). waybill's curated
table already matches those exact CPEs on 14 of 16 openssh-closure components;
it differs on bash (`5.3p9` in the version field vs nixpkgs' `5.3` + update `9`)
and lacks gnugrep.

### 3. waybill's SBOM subject is wrong for a flake scan
`metadata.component` = `filesystem-scan 0.0.0`, CPE
`cpe:2.3:a:waybill:filesystem-scan:0.0.0`; the `openssh 10.3p1` component has no
CPE. sbomnix's subject is `openssh 10.3p1`, `cpe:2.3:a:openbsd:openssh:10.3p1`.

### 4. Go modules and Rust crates are invisible to both closure SBOMs
Neither tool lists a `pkg:golang` for gh or a `pkg:cargo` for ripgrep, in any
mode; grype matched nothing on `go` or `gh-2.93.0-go-modules`. Pointed at the
**realised output**, waybill's existing binary readers recover **163 Go modules
from gh, and grype finds 55 vulnerabilities on that SBOM** (grype's own binary
scan: 40 on 10 packages). Not adjudicated. ripgrep: 45 crates read via
cargo-auditable, then see §6.

### 5. Runtime is a fraction of buildtime
Nix runtime closures: curl 20, openssh 11, httpie 66, gh 5, ripgrep 3 paths,
against 457–2,457 buildtime components. sbomnix's runtime findings are 288 of
1,475 (20%); the rest are build tooling that never ships. waybill has no runtime
mode. sbomnix's runtime SBOM for openssh has **0 components** for an 11-path
closure (sbomnix-side anomaly, not investigated).

### 6. Bug found: debug-build panic on cargo-auditable binaries
`builder.rs:~1744` `debug_assert!` allowlist omits `cargo-auditable`, which
`scan_fs/binary/mod.rs:59` `BINARY_EVIDENCE_KINDS` declares. Debug builds panic
(exit 101) on any cargo-auditable binary; release builds are unaffected. The one
and only kind out of sync.

### Not established
Six targets, one revision, one system. moat runtime not measured (unrealised).
Runtime relevance is a name match against Nix's runtime closure, so a same-named
package can be misattributed (Haskell `zlib` 0.7.1.1 vs C zlib). Go-module findings
not adjudicated. #1283 (callCabal2nix) not exercised: no such target in the set.

## Probe errors caught on the way (each produced a plausible wrong figure)
1. Relative flake path → `nix-closure-paths=0` for every target.
2. derivation-show **format v4** nests under `derivations`; reading the wrapper as
   a derivation gave "0 patched-by-nix". Caught by the unzip positive control.
3. "curl ≡ gh grype findings" — real, not a bug: every hit on gh's build closure
   is fetch/stdenv tooling shared with curl's.
