# Measurements for 1035-nix-closure-sbom

Probes are committed so the findings are reproducible and so the next
person can re-run them when nixpkgs moves. An undocumented property of an
external system is one that can change without notice.

## `closure-vs-emitted.py`, `patch-attribution.py`

The two probes the spec was written from, against two Haskell projects.
Their headline counts are **per derivation**. The shipped implementation
merges build variants, so its component and patch counts are per
identity and are legitimately lower — 111 patch-applying derivations
collapse to 61 components, and 336 patch entries to 187. The distinct-CVE
count is the same either way, which is how the two were reconciled.

## `closure-composition.sh` — T-R2

**Question**: the spec's role split and patch density were measured on two
Haskell projects. Do they generalise?

**Answer: partly, and the part that does not matters.**

Run 2026-09-29 against nixpkgs, one representative package per ecosystem:

| flakeref | drvs | artifact | tooling | both | unref | patches | no CVE |
|---|---|---|---|---|---|---|---|
| `ripgrep` (Rust) | 930 | 47 | 89 | 12 | 782 | 92 | 89 |
| `hello` (C) | 537 | 14 | 34 | 4 | 485 | 67 | 64 |
| `jq` (C) | 563 | 14 | 35 | 4 | 510 | 68 | 65 |
| `gopls` (Go) | 837 | 27 | 67 | 7 | 736 | 86 | 83 |
| `python3Packages.requests` | 952 | 46 | 96 | 18 | 792 | 93 | 90 |
| *moat (Haskell), for comparison* | 1275 | 264 | 134 | 52 | 825 | 187 | 167 |

What holds:

- The four roles are populated everywhere; the classifier is not reading a
  Haskell-specific convention.
- The unreferenced majority holds — 84% to 91% across all of them.
- Patches are everywhere, and most name no CVE everywhere.

What does **not** hold:

- **The artifact-to-tooling ratio inverts.** Haskell has roughly twice as
  many artifact inputs as build-tooling members (264 vs 134); every other
  ecosystem measured has the reverse, by two to three times. A Haskell
  closure carries its library dependencies as derivations, while a Rust or
  Go closure has them vendored inside one build step and shows mostly
  toolchain. Any future guidance that leans on "most of a closure is
  artifact inputs" would be a Haskell observation stated as a nix one.
- **The no-CVE fraction is worse elsewhere, not better.** Haskell measures
  89–91%; these measure 96–97%. The coverage limit the documentation
  states is the optimistic end of the range.

Caveat, stated because it bounds the claim: these are nixpkgs leaf
packages, not projects' own flakes. That is the right comparison for
composition — a `buildRustPackage` derivation has the same closure shape
wherever it is defined — but a real Rust project's flake would add its own
dependency derivations and could move the ratio back. Re-run the probe
against a real non-Haskell project flake before quoting these as the last
word.

## `closure-cold-cost.sh` — T-R3

**Question**: what does the tier cost on a store that has never seen the
inputs?

**Not yet answered.** The probe is committed and reports both figures, but
a truthful cold number needs a runner whose Nix store is genuinely empty,
and no such runner was available here. The probe will not fake one: garbage
collecting a shared store to take a measurement destroys everything else on
the machine, and a warm figure relabelled as cold is worse than no figure.
`WAYBILL_COLD=1` records the operator's claim in the output so a warm run
can never be mistaken for a cold one after the fact.

Until it runs, the documentation quotes the warm figures and says so.
