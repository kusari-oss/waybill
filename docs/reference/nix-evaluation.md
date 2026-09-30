# Nix evaluation options (waybill sbom scan)

Milestone 1034 (issue #971) adds three flags to `waybill sbom scan` that
resolve a Nix-built project's package versions by evaluating Nix instead
of parsing nixpkgs files. All three are opt-in — the default behavior
(fetch and parse `hackage-packages.nix` at the pinned revision) is
unchanged, and with the flag absent no `nix` process is started at all.

| Flag | Purpose | Repeat? |
|---|---|---|
| `--nix-eval` | Resolve versions by evaluating nixpkgs at the revision `flake.lock` pins. **Executes code.** | no (boolean) |
| `--nix-eval-system <SYSTEM>` | Platform to evaluate for, e.g. `x86_64-linux`. Defaults to the host's. Requires `--nix-eval`. | no |
| `--nix-eval-timeout-secs <N>` | Wall-clock budget for evaluation. `0` is rejected. Requires `--nix-eval`. | no |
| `--nix-closure` | Read the **derivation closure** — everything Nix builds the project from, including the C toolchain and the patches applied to it. **Executes code.** | no (boolean) |
| `--nix-closure-attr <ATTR>` | Which flake attribute's closure to take. Defaults to `default`. Requires `--nix-closure`. | no |

> **`--nix-eval` executes code.** Every other `sbom scan` flag reads
> files. Run it inside a sandbox, or only against a flake you trust.

Complements — does not replace — the default nixpkgs resolution
controlled by `--no-nixpkgs-haskell` and `--no-nixpkgs-haskell-closure`.

## When to use it

| Situation | Flag |
|---|---|
| You need versions that match what Nix actually builds, and can run `nix` in a sandbox or trust the flake | `--nix-eval` |
| You are producing an SBOM for a platform other than the scan host | `--nix-eval-system <system>` |
| Evaluation is being killed by the default budget on a large flake | `--nix-eval-timeout-secs <n>` |
| You want the components and patches Nix actually builds with, not only the ones the language manifest declares | `--nix-closure` |
| The flake exposes several outputs and you want a specific one | `--nix-closure-attr <attr>` |
| You cannot execute code from the scanned tree, or `nix` is unavailable | no flags — file parsing is the default |

`--nix-eval` and `--offline` are mutually exclusive in effect. Evaluation
resolves the pinned revision through `getFlake`, which fetches it when the
Nix store does not already hold it, so the tier cannot honour a promise of
no outbound network calls. `--offline` wins: the tier is skipped and
`waybill:nix-eval-degraded` records `offline-requested`.

Parsing nixpkgs files reconstructs what Nix *would* compute. Evaluating
asks Nix what it *does* compute. Issue #1033 shipped two wrong versions
because `configuration-common.nix` — a file that supersedes the
generated package set — was not being read. Evaluation does not have
that failure mode.

Four things only evaluation can establish:

| | Why file parsing cannot see it |
|---|---|
| Package-set overrides | Spread across files that override each other in evaluation order |
| The compiler a flake selects | Chosen by expressions, not declared |
| `meta.knownVulnerabilities` | An attribute on the derivation, absent from the version tables |
| The derivation closure | Does not exist until something is evaluated |

## Worked examples

### Resolve against the pinned revision

```sh
waybill sbom scan --path ./my-haskell-project --nix-eval \
  --format cyclonedx-json --output cyclonedx-json=out.cdx.json

jq '.metadata.properties[] | select(.name | startswith("waybill:nix-eval"))' out.cdx.json
# waybill:nix-eval-tier   = {"degraded-reason":null,"evaluated":21,"revision":"a799d3e3…","superseded":0,"system":"aarch64-darwin"}
# waybill:nix-eval-system = aarch64-darwin
```

### Produce an SBOM for a different platform

Results are platform-specific: `hinotify` is 0.4.2 on `x86_64-linux`
and 0.1.8 on `aarch64-darwin` at the same nixpkgs revision. The
document records which platform it describes.

```sh
waybill sbom scan --path ./my-haskell-project --nix-eval \
  --nix-eval-system x86_64-linux --output out.cdx.json
```

### Find which versions were checked against Nix

```sh
jq -r '.components[]
       | select(.purl | startswith("pkg:hackage/"))
       | [.name, (.properties[]? | select(.name=="waybill:nix-eval-origin") | .value)]
       | @tsv' out.cdx.json
# text        evaluated
# QuickCheck  file-parsed
```

## What runs

Three `nix` invocations per scan, and no others:

```sh
nix config show --option allow-import-from-derivation false   # verify the refusal
nix eval --impure --raw --expr builtins.currentSystem         # host platform
nix eval --json  --option allow-import-from-derivation false --expr '<versions>'
```

The expression evaluates
`(builtins.getFlake "github:NixOS/nixpkgs/<rev>").legacyPackages.<system>`.
The project's **own** flake is not evaluated, so expressions the scanned
repository authors do not run.

> **This is true of `--nix-eval` only.** `--nix-closure` (milestone 1035) takes
> a derivation closure, which cannot be obtained without instantiating the
> project's flake — so under that flag, repository-authored expressions *do*
> run. The defences below apply to both paths and become load-bearing rather
> than precautionary under `--nix-closure`. The code that does run is nixpkgs —
third-party, and not audited by waybill. The repository chooses which
revision of it; that revision must be a 40-character hex object id or
the tier declines to use it.

## Failure diagnostics

Every failure degrades to file parsing and the scan still succeeds. The
reason appears at document scope in `waybill:nix-eval-degraded`.

### Case 1 — `offline-requested`

`--offline` was set. The tier cannot guarantee it will not fetch the pinned
revision, so it does not run. Drop `--offline` if you want evaluation.

### Case 2 — `tool-absent`

No `nix` on `PATH`. Install Nix, or drop the flag.

### Case 3 — `tool-unusable`

`nix` is present but the daemon or store is not. Check the daemon.

### Case 4 — `ifd-refusal-unverified`

This `nix` will not honour `allow-import-from-derivation`. waybill
declines to evaluate rather than evaluate unprotected. Upgrade Nix.

### Case 5 — `revision-unfetchable`

The pinned revision could not be acquired, or `flake.lock` does not
pin a 40-character hex revision. Check network access and the lockfile.

### Case 6 — `no-evaluable-attribute`

The flake exposes nothing the tier can use. Expected for some flakes:
haskell-language-server exposes no `default` package, only `docs` and
devShells.

### Case 7 — `evaluation-failed`

`nix` exited non-zero. The scan log carries its stderr.

### Case 8 — `budget-exceeded`

Evaluation outlasted its budget. Raise `--nix-eval-timeout-secs`.

`waybill:nix-eval-tier` records what the tier did whenever it ran,
degradation included — so "ran and got nothing" stays distinguishable
from "never ran".

## Coverage

The tier evaluates in two passes: declared dependencies first, so the
transitive walk starts from corrected data, then the packages that walk
reached. Measured coverage on two real Haskell libraries is 51 of 53 and
190 of 190 components.

What stays `file-parsed` is the scanned project itself. Asking nixpkgs
about it would be meaningless at best and wrong at worst — a project
shares its name with whatever nixpkgs publishes under that name, and
those are different artifacts. One measured case: the local source is
2.2.2.0 while nixpkgs carries 2.2.0.0 under the same name.

Every component carries `waybill:nix-eval-origin`, so which is which is
visible in the document rather than inferred.

On every project tested so far, file parsing and evaluation agreed on
every component. The tier has not yet corrected a version outside the
case that motivated it. It closes a gap in checking, and that value is
prospective.

## Cost

Measured on a warm Nix store:

| | |
|---|---|
| Evaluating 21 names | ~0.5s |
| Evaluating 423 names | ~0.5s |
| nixpkgs source in the store | 300–335 MB per revision |
| The file-parsing path, for comparison | 16 MB per revision |

The first scan on a machine that has never fetched the pinned revision
pays that 300 MB, and it is paid **inside** the evaluation budget: the
fetch happens within the `nix eval` call, so `--nix-eval-timeout-secs`
has to cover both. On a cold store the budget is therefore partly a
bandwidth allowance, which is why the default is generous. Raise it, or
warm the store first with `nix flake prefetch`, if a first scan degrades
with `budget-exceeded`.

## The derivation closure (`--nix-closure`)

`--nix-eval` corrects versions of packages the project's manifest already
declares. `--nix-closure` answers a different question: what does Nix
actually build this from? It runs `nix derivation show -r` on the selected
attribute and reads the result.

Measured on two Haskell projects, that is 1,275 and 1,535 derivations,
emitting 348 and 486 components after merging build variants. Almost none
of them appear in the cabal files. They are the C toolchain, the build
tooling, and the libraries those link against.

### What it adds

Closure components **supplement** the manifest-derived set; they never
replace it. The two answer different questions, and neither is a subset of
the other. A manifest covers every stanza the project declares, while a
closure covers only what the selected attribute builds — measured, one
project's executable-stanza dependencies and its entire GHC boot library
set are absent from its library's closure, so treating closure-absence as
evidence of spuriousness would discard the Haskell standard distribution.

Each component carries `waybill:closure-role`:

| Role | Meaning |
|---|---|
| `artifact-input` | Reached through `buildInputs` — goes into the artifact |
| `build-tooling` | Reached through `nativeBuildInputs` — builds it, is not part of it |
| `both` | Referenced both ways |

A closure member that the manifest readers already found contributes its
role and patches to that component rather than becoming a second entry —
the closure identifies members as `pkg:generic/<name>@<version>`, since
`pkg:nix` is not a purl-spec type, and a generic twin beside an existing
`pkg:hackage/…` would leave the document asserting two identities for one
package. Measured on one project: 32 members merged that way and 316 were
genuinely new.

One residue is left deliberately. Matching is on name *and* version, so a
component whose version never resolved does not absorb a closure member of
the same name — measured, one of 370 components appears as both
`pkg:hackage/os-string` (design tier, no version) and
`pkg:generic/os-string@2.0.10` (build tier). Those are two different
observations, what the manifest declares and what the build used, and
collapsing them would assert the manifest resolved something it did not.

Members referenced by neither are not emitted, with one exception: those
that apply a patch are. Measured, `jq` and `lua` are unreferenced in one
project's closure and carry 6 of its 18 CVEs, so dropping them would leave
those patches with no component to attach to.

### Backported patches

This is the part no other tier can reach. nixpkgs backports security fixes
without moving a version string. `unzip` is 6.0 in both measured closures —
unchanged since 2009 — and carries 11 CVEs across its patch set. A
version-keyed SBOM cannot say "this build is patched", and cannot say "this
version was considered vulnerable" either.

CycloneDX has a native carrier and gets one:

```json
"pedigree": { "patches": [
  { "type": "backport",
    "resolves": [ { "type": "security", "id": "CVE-2019-13232" } ] }
] }
```

Neither SPDX version has an equivalent, so both carry the same array in a
`waybill:closure-patches` annotation. That is the one place the three
formats differ in capability rather than in spelling.

A backport also produces **two** OpenVEX statements, never one: `affected`
for the component version as published, and `not_affected` for the build
this document describes, with the component named as a subcomponent. Both
carry the evidence grade. A lone `not_affected` would let a consumer
suppress a real finding on the weaker half of the evidence.

### Coverage, and its limit

The CVE is read out of the patch filename. That is evidence the maintainers
believed the version vulnerable; it is not proof the patch fully resolves
the issue, and a backport whose filename names no CVE is invisible to it.
Every CVE association therefore carries
`waybill:patch-evidence-grade = filename-derived`, and a future stronger
provenance will be distinguishable from it rather than indistinguishable.

**Most backports name no CVE.** Measured, 167 of 187 patches on one project
and 153 of 169 on the other — 89% and 91%. The counts are emitted at
document scope in `waybill:nix-closure` precisely so that absence of a VEX
statement does not read as absence of a backport:

```sh
waybill sbom scan --path . --nix-closure --format cyclonedx-json --output sbom.json
jq -r '.metadata.properties[]|select(.name=="waybill:nix-closure").value' sbom.json
# {"attribute":"default","components-emitted":348,"derivations":1275,
#  "distinct-cves":18,"patches":187,"patches-without-cve":167,
#  "roles":{"artifact-input":264,"both":52,"build-tooling":134,
#           "unreferenced":825}}
```

The attribute is in that record because it is load-bearing: two attributes
of one flake yield different closures, so the counts cannot be read without
knowing which was taken.

### Finding the patched components

```sh
jq -r '.components[]|select(.pedigree)
       |"\(.name)@\(.version) \([.pedigree.patches[].resolves[]?.id]|unique|join(","))"' \
  sbom.json | grep CVE
# unzip@6.0 CVE-2014-8139,CVE-2014-8140,…,CVE-2019-13232,CVE-2021-4217
```

### Cost and failure

`--nix-closure` degrades with the same reason codes as `--nix-eval` (see
**Failure diagnostics** above) and refuses outright under `--offline`:
resolving a flake reference fetches, and Nix's own `--offline` governs
substituters rather than flake inputs, so the tier declines rather than
reaching the network behind the flag. With the flag absent, no `nix`
process starts and none of the closure annotations is emitted.

## Security guidance

**Prefer a sandbox.** Either flag starts an evaluator on a tree you are
analyzing. Treat it the way you would treat running the project's build.

The two differ in what they evaluate, and it matters:

- `--nix-eval` evaluates **nixpkgs** at the revision `flake.lock` pins. It
  reads the scanned repository's lockfile but does not evaluate the
  repository's own expressions.
- `--nix-closure` evaluates the **project's own flake**, because
  instantiating a derivation requires it. Expressions authored in the
  repository run.

Defences are on by default and not configurable:

- **Pure mode.** No `--impure` on the resolving call, so the host
  environment is unreadable (`builtins.getEnv "HOME"` returns `""`).
  Passing the platform explicitly is what makes this possible;
  `builtins.currentSystem` does not exist in pure mode.
- **Import-from-derivation refused**, so evaluation cannot build
  anything or run a builder.
- **The refusal is verified, not assumed.** A `nix` that does not
  support the setting accepts the flag, warns, and exits 0 — asking is
  not evidence of getting. waybill checks `nix config show` reflects it.
- **A wall-clock budget.** Nix bounds recursion depth but not time.
- **No writes** outside the Nix store and waybill's own cache.
- **The flake is never allowed to configure Nix.** A flake can request
  settings of its own through `nixConfig`, including
  `allow-import-from-derivation` — real ones do; `slack-web` sets it.
  Nix ignores such requests as untrusted unless `--accept-flake-config`
  is passed, which waybill never passes. Measured: a flake carrying that
  setting is refused by waybill's invocation and builds its derivation
  once `--accept-flake-config` is added.

Those defences apply to both flags. Import-from-derivation is refused on
the closure call too, so the project's flake can be evaluated but cannot
build anything or run a builder during evaluation.

An earlier version of this section described taking the derivation closure
as a possible future step, and said that if it landed, repository-authored
expressions would run. It landed, in milestone 1035, and they do. The
sandbox advice above is no longer precautionary for `--nix-closure`; it
describes the actual exposure. `--nix-eval` remains the narrower of the
two, and nixpkgs is still code waybill does not audit.

## Related

- [SBOM format mapping](./sbom-format-mapping.md) — rows C177 through C185
- [Reading a Waybill SBOM](./reading-a-waybill-sbom.md)
