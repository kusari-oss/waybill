# Nix evaluation options (waybill sbom scan --nix-eval)

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
repository authors do not run. The code that does run is nixpkgs —
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

## Security guidance

**Prefer a sandbox.** Enabling `--nix-eval` starts an evaluator on a
tree you are analyzing. Treat it the way you would treat running the
project's build.

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

The guidance is stricter than today's behavior for two reasons. nixpkgs
is code waybill does not audit, and whether a given attribute triggers
import-from-derivation has not been measured. And the useful next step —
taking the full derivation closure via `nix derivation show -r`, which
yields far more (1,275 derivations and 380 distinct name/version pairs
on one Haskell library, against 53 components today) — instantiates the
project's own flake. If that lands, repository-authored expressions will
run and this section will describe literal behavior.

## Related

- [SBOM format mapping](./sbom-format-mapping.md) — rows C177 through C181
- [Reading a Waybill SBOM](./reading-a-waybill-sbom.md)
