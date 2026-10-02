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


## Q4 — can a closure member's declaration be reached at all, and how often?

`attribute-coverage.sh`. Added during planning, because the spec assumed a
mechanism that did not exist: `meta` is absent from every one of moat's 1,275
derivations.

Resolving each member's `pname` across an ordered list of package sets and
verifying by output path, against the project's own pinned nixpkgs:

| | count | share |
|---|---:|---:|
| members (distinct pname + version) | 380 | |
| confirmed by output path | 273 | 71% |
| no attribute in any probed set | 72 | 18% |
| attribute found, output path differs | 35 | 9% |

Confirmed by set: haskell 118, python3 69, top-level 86, perl 0. **Top-level
alone reaches only 86 (22%)** — the nested sets are most of the coverage, not
a refinement.

Evaluation of all 376 names takes **0.6–1.1 s** in one expression.

Two findings the implementation depends on:

- **The 9% is the check earning its keep.** Those members have an attribute of
  the right name that builds something else. Accepting them would attach a
  security claim to the wrong component.
- **Zero confirmed members carry a declaration**, and that is structural. Nix
  refuses to *evaluate* a package marked insecure, so a project that builds
  has already permitted any it contains. The feature fires only where an
  operator accepted an exception, so an empty result is the common case and
  needs to be distinguishable from a failed one.

Three traps cost a measurement round each and are commented in the probe:

- The closure JSON omits the `/nix/store/` prefix that `outPath` carries.
  Comparing raw gives **0%** and reads as "the mechanism does not work".
- `or null` is needed on the attribute lookup: a missing attribute is not a
  throw and escapes `tryEval`.
- `deepSeq` must be *inside* `tryEval`, which returns a lazy value — otherwise
  the throw escapes at serialisation time and the first unfree package kills
  the run.


## Q5 — what does the pass cost, and can that cost be absorbed?

SC-006a asked for the added wall-clock cost of a `--nix-closure` scan, with
the rule that a figure over roughly a fifth of the existing closure-scan time
reopens the decision to run automatically. It does.

Three consecutive runs against moat (1,275 derivations, 376 closure members),
everything warm, timed by an `Instant` around each phase rather than by
toggling a flag — a toggle cannot distinguish "the pass is slow" from "the
pass changed what a later phase does":

| phase | run 1 | run 2 | run 3 |
|---|---:|---:|---:|
| closure query + classify | 388 ms | 388 ms | 388 ms |
| declarations pass | 706 ms | 706 ms | 706 ms |
| whole scan, wall clock | 2.91 s | 1.25 s | 1.25 s |

Run 1 is cold-start; runs 2 and 3 agree.

**The pass costs more than the closure query it rides on.** Against a
pre-feature scan of roughly 550 ms it adds 706 ms — it does not disappear
into the existing work, it roughly doubles it. That is six times the
threshold, not a margin to argue about.

Two corrections fall out of this:

- A comment at the call site claimed "~1.6s against a closure scan measured
  in tens of seconds". Both halves were wrong. The closure query is 388 ms
  warm, and the ratio that comment implied — a rounding error — was the
  entire basis for running the pass with no way to decline it. The figure
  appears to have been carried over from an early cold-eval observation and
  never re-measured against the phase it was being compared to.
- `--no-nixpkgs-security` now exists. The default still runs, because the
  absolute cost is under a second and the pass is silent on projects with
  nothing to declare, but an operator who wants only the composition is no
  longer required to pay for the security read.

The per-phase `elapsed_ms` fields on both log lines are kept, so the next
person gets the number from any scan rather than re-deriving it.


## Q6 — the feature could not fire, and the coverage figure was hiding it

Added after milestone 1050 merged, when the shipped binary was run against
real repositories for the first time. Two defects, both of which presented
as a *coverage number* rather than as a failure, and which masked each other.

### The probe could not read an insecure package

`meta.knownVulnerabilities` and `outPath` were read inside one `tryEval`:

```nix
raw = if cand == null then null else { out = cand.outPath; kv = cand.meta.knownVulnerabilities or []; };
r   = builtins.tryEval (builtins.deepSeq raw raw);
```

Measured against moat's pinned revision:

| read | result |
|---|---|
| `alist.meta.knownVulnerabilities` | `success: true` — both strings returned |
| `alist.outPath` | **`success: false`** |

Nix refuses to *evaluate* the derivation of a package marked insecure. The
probe reads the output path to verify the candidate, so the throw discarded
the candidate — and the packages that throw are **exactly** the packages
that carry declarations. The feature could not fire on the only case it
exists for.

Fixed by importing with the gates open
(`config.allowInsecurePredicate = _: true; allowUnfree = true;`). These
govern whether evaluation is *permitted*, not what is built: measured,
`hello.outPath` is byte-identical under this config and under plain
`legacyPackages`, so verification keeps its meaning.

**Why it survived review.** Every real scan reported `declarations=0`, and
the documentation explained that as "the common case, because a building
project has already permitted what it contains" — true, and indistinguishable
from the bug. The fixture deliberately carried no declaration (R8), so no
test exercised the fire path. A passing suite and a feature that can never
fire looked identical.

### A wrong attribute shadowed the right one

The probe took the first set whose attribute *existed*, not the first whose
output path *matched*. moat's closure contains the Haskell library `lens`;
top-level `pkgs.lens` is `lens-desktop`, an unrelated application. Under the
default config that attribute happened to throw (unfree), so the search fell
through to `haskellPackages.lens` and confirmed — by luck. With the gates
open it evaluates, the search stops, verification correctly rejects it, and
the real match is never tried.

Fixed by returning every set's candidate in precedence order and selecting
the first whose output path matches.

### Both fixes, measured

| | before | gates open | + first-match |
|---|---:|---:|---:|
| moat, members checked | 273 | 272 | **280** |
| slack-web, members checked | — | 391 | **407** |
| control (accepts `alist`), declarations | 0 | 2 | 2 |
| control (accepts `dcraw`), distinct CVEs | — | 5 | 5 |

The shadowing bug alone cost 7 confirmable members on moat and 16 on
slack-web. Coverage on moat is now 280/376 (74%), not 273/376 (72%).

Cost is unchanged in practice. A first reading of 3443 ms was cold cache
from a fresh release build; three warm runs give **983 / 935 / 934 ms**
against a 387 ms closure query, versus 917 ms before. Correctness cost
about 2%, not 3.7×.

### What the controls are

Two throwaway flakes, in `measurements/`, each pinning moat's nixpkgs
revision and accepting one package nixpkgs marks insecure via
`permittedInsecurePackages`:

- `control-insecure` — `alist`, whose declarations name no CVE;
- `control-cve` — `dcraw`, which names CVE-2018-19655 and CVE-2018-19565.

Building either requires the right version string: `alist-3.57.0`, not a
guessed one. Nix refuses to evaluate otherwise, which is itself a live
confirmation of the Q3 gate finding.

## Q7 — which prose is publishable

57 attributes in this revision carry `knownVulnerabilities`; 27 distinct
entries name no CVE. Classified by hand:

| kind | count | example |
|---|---:|---|
| a component inside the package | 5 | "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal, and proj" |
| lifecycle of the software itself | ~18 | "minio has been abandoned by upstream" |
| **a claim about a party** | **2** | "… was acquired by \<company\>, a company distrusted by the community"; "Please nag \<company\> to update to OpenSSL 3" |

The spec deferred the party claims. They are now **withheld** — read,
counted, and logged, but not emitted — while composition and lifecycle
entries are published.

The filter matches the party class rather than allow-listing the
composition one, because an allowlist of "bundled|vendors|includes" would
also drop "abandoned by upstream", which is wanted. New upstream phrasing
will therefore be *emitted* rather than withheld: that is the chosen failure
direction, since over-withholding silently discards the bundled-component
declarations that are the reason prose is carried at all. The test pins the
rule against all 27 measured strings, in both directions.

**A gap this classification found.** `nexus` declares `Sonatype-2015-0286`
and `Sonatype-2022-6438` — real advisory identifiers that are not CVEs. The
identifier pattern matches `CVE-\d{4}-\d+` only, so these are filed as prose
and reach the SBOM annotation rather than VEX, where OpenVEX would accept
them as vulnerability names. Not changed here; widening the pattern is its
own decision.

## Q5 — which non-CVE identifier shapes actually occur? (#1051)

`kv-identifier-census.py`. A **whole-tree static census** rather than an
evaluated sample: every string literal inside every
`knownVulnerabilities = [ … ]` list under `pkgs/`, at
`a799d3e3886da994fa307f817a6bc705ae538eeb` (the revision #1051 cites, via
`nix flake prefetch`). Run 2026-10-02.

| | |
|---|---:|
| files declaring the attribute | 79 |
| string-literal entries | 159 (153 distinct) |
| naming a CVE | 120 |
| prose | 36 |
| `PREFIX-YYYY-N` — `Sonatype-2015-0286`, `Sonatype-2022-6438` | 2 |
| containing GHSA ids — `django-ckeditor`, three in one sentence | 1 |
| prose entries containing any other `<word>-<digits>` token | **0** |

So the shapes that occur are CVE, GHSA and one vendor `PREFIX-YYYY-NNNN`
form, and a pattern covering exactly those matches nothing in the prose at
this revision. The GHSA entry is a second instance of the gap above.

Static, so literals only: `lib.optional cond "…"` forms are not counted.
Five were found by hand (Garage, Electron, Netbox, Lima, Xen), all EOL prose
interpolating `${version}`, none an identifier.

Not implemented: the identifiers also feed C188's `distinct-cves` and
`declarations-without-cve` wire fields, so widening the pattern changes what
a consumer-visible field means. Options are recorded on #1051.
