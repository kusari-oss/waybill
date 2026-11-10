# How a CPE's version encoding changes what grype matches — 2026-11-09

grype 0.120.0, vulnerability DB built 2026-11-08. Each `*.cdx.json` here is a
one-component CycloneDX document differing only in the CPE; the matching
`*.grype.json` is grype's output for it (`grype sbom:<file> -o json`).

## bash 5.0, patch level 11

| file | CPE | grype vulnerabilities |
| --- | --- | --- |
| `waybill-version-field` | `cpe:2.3:a:gnu:bash:5.0p11:*:…` (waybill's curated form) | **0** |
| `nixpkgs-update-bare` | `cpe:2.3:a:gnu:bash:5.0:11:…` (nixpkgs' `meta.identifiers` form) | 1 — CVE-2019-18276 |
| `nvd-update-patchN` | `cpe:2.3:a:gnu:bash:5.0:patch11:…` (NVD's form) | 1 — CVE-2019-18276 |
| `version-only` | `cpe:2.3:a:gnu:bash:5.0:*:…` | 1 — CVE-2019-18276 |

## openssh

| file | CPE | grype vulnerabilities |
| --- | --- | --- |
| `openssh-nixpkgs-version-p1` | `openbsd:openssh:10.3p1` (nixpkgs' form) | 28 |
| `openssh-update-p1` | `openbsd:openssh:10.3:p1` (NVD's form) | 28 |
| `openssh-old-version-p1` | `openbsd:openssh:9.3p1` | 40 |
| `openssh-old-update-p1` | `openbsd:openssh:9.3:p1` | 41 |

## NVD CPE dictionary, queried 2026-11-09

`https://services.nvd.nist.gov/rest/json/cpes/2.0?cpeMatchString=<prefix>`. The
update-field values found under each version:

| prefix | entries | update-field values |
| --- | --- | --- |
| `cpe:2.3:a:gnu:bash:5.3` | 0 | — |
| `cpe:2.3:a:gnu:bash:5.2` | 7 | `-`, `alpha`, `beta`, `rc1`–`rc4` |
| `cpe:2.3:a:gnu:bash:5.0` | 15 | `-`, `beta1`, `beta2`, `rc1`, `patch1`–`patch11` |
| `cpe:2.3:a:gnu:bash:4.4` | 25 | `*`, `-`, `beta1`, `beta2`, `rc1`, `rc2`, `patch1`–`patch19` |
| `cpe:2.3:a:openbsd:openssh:9.3` | 3 | `-`, `p1`, `p2` |

So NVD puts a bash patch level in the update field as `patchN`, and an openssh
portable release as `pN`. Neither convention puts the suffix in the version
field. nixpkgs follows that for bash (update `9`, not `patch9`) but not for
openssh (version `10.3p1`).

## Reproduce

```sh
G=$(nix build --no-link --print-out-paths nixpkgs#grype)/bin/grype
for f in *.cdx.json; do "$G" "sbom:$f" -o json --file "${f%.cdx.json}.grype.json"; done
```
