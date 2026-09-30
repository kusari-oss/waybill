# Measurements — nixpkgs `meta.knownVulnerabilities`, against OSV

Run 2026-09-30, aarch64-darwin, nixpkgs from the flake registry.

These precede any spec, per CLAUDE.md. The probes are committed so the
findings are reproducible and so they can be re-run when nixpkgs or OSV
moves — an undocumented property of an external system is one that can
change without notice.

## Q1 — are the entries CVE identifiers, or prose?

`known-vulnerabilities.sh`. Sampled the 46 `pkgs/by-name` attributes that
declare the attribute, since the attribute name is the directory name there
and the sample is addressable without evaluating all of nixpkgs.

| | |
|---|---:|
| attributes probed | 46 |
| evaluated cleanly | 45 (`ovftool` needs unfree-license acceptance) |
| declaring a non-empty list | 35 |
| entries | 76 |
| **CVE-bearing** | **55 (72%)** |
| **prose, no CVE** | **21 (28%)** |
| distinct CVE ids | 63 |

This is a sample of `by-name` only — 46 of the ~90 files that declare the
attribute. Do not quote it as a census.

## Q1a — the prose entries are about a different layer, not softer CVEs

This is the finding worth acting on. The prose is overwhelmingly about
**bundled components and upstream abandonment**:

- `googleearth-pro` — "Includes vulnerable versions of bundled libraries:
  openssl, ffmpeg, gdal, and proj."
- `cypress` — "Uses Electron 37.6.0, EOL on October 4, 2025, Several CVEs
  known." (four packages carry an Electron-EOL entry)
- `minio` — "abandoned by upstream and security issues won't be fixed"
- `fspy` — "Vendors Electron 2.0 (end-of-life)"

An SBOM of `googleearth-pro` lists `googleearth-pro`. The vulnerable
`openssl` is *inside* it and has no component of its own, so no version
matcher can fire on it, and no CVE feed will ever say "the thing you are
building on is unmaintained". That is a first-party maintainer judgement
about a layer the component graph does not reach.

One entry is a supply-chain trust claim rather than a vulnerability at all
(`alist` — "acquired by [a company] distrusted by the community"). Emitting
that is a separate decision and probably a contentious one.

## Q2 — does OSV already know this, and can a Nix scan ask it?

`osv-coverage.py`.

**A method error first, because it inverted the answer.** The first pass
queried OSV by CVE id (`/v1/vulns/CVE-…`) and found 17 of 63 missing and 42
of the remaining 46 carrying no package or ecosystem — which reads as "OSV
barely covers this". That was wrong. OSV's ecosystem advisories have their
own ids (`GHSA-`, `PYSEC-`) and carry the CVE only as an *alias*; querying
by id returns the NVD-derived record, which routinely has
`affected[0].package == null`. **A scanner queries by package.** Re-queried
that way:

```
PyPI/alerta-server@9.0.4        2   GHSA-8prr-286p-4w7j, PYSEC-2026-2341
Debian:12/unzip                19   DEBIAN-CVE-2003-0282, …
Alpine:v3.20/unzip             14   ALPINE-CVE-2014-8139, ALPINE-CVE-2014-8140, …
Ubuntu:22.04/unzip              4   UBUNTU-CVE-2021-4217, …
pkg:generic/unzip@6.0           0
```

So:

- **Language ecosystems: OSV is complete and nixpkgs adds nothing.**
  `alerta-server`'s CVE is in OSV with version ranges.
- **System packages: OSV is complete _per distro_, and Nix is not one of
  the distros.** There is no `Nix:<release>` ecosystem.
- **`pkg:generic/unzip@6.0` — the identity waybill emits for a Nix closure
  component — matches nothing.**

### The example that settles it

Alpine files `ALPINE-CVE-2014-8139`, `-8140` and `-8141` against `unzip`.
Milestone 1035 measured Nix's own `unzip 6.0` applying patches named
`CVE-2014-8139.diff`, `CVE-2014-8140.diff` and `CVE-2014-8141.diff`.

Same upstream version, different patch state. Borrowing a neighbouring
distro's advisories for a Nix build would report those three as present
when the build patches them. nixpkgs' security metadata is not a weaker
OSV — **it is the distro-specific data OSV would carry if Nix were an
ecosystem in it**, and the patch evidence is the other half of the same
record.

## Q3 — is `permittedInsecurePackages` observable?

`insecure-gate.sh`. **It does not need to be.** Nix refuses to *evaluate* a
package carrying `knownVulnerabilities`:

```
Refusing to evaluate package 'cypress-15.19.0' … because it is marked as insecure
```

A derivation for one therefore cannot reach a closure unless permission was
granted. **Presence in the closure is the acceptance signal**; waybill never
has to locate the config that granted it.

Two caveats worth carrying into a spec:

- `NIXPKGS_ALLOW_INSECURE=1` permits everything at once, so presence proves
  *some* permission was granted, not a targeted one. The statement waybill
  can support is "this build accepted a package nixpkgs marks insecure", not
  "the operator named this package".
- Conversely, absence proves nothing about intent — the package may simply
  not be a dependency.

A first attempt at this probe used `checkinstall`, which is Linux-only, and
got an "unsupported for this system" refusal that looks like a confirmation
if you only check that the command failed. The probe now asserts on the
message, not the exit status.
