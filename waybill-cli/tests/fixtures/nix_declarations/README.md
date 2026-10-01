# `nix_declarations`

A minimal Nix project for the paths that do **not** need a declaration:
degradation, byte-identity, and coverage reporting.

## What this fixture cannot do, and why

An earlier draft defined packages carrying `meta.knownVulnerabilities` in
this flake and expected them to drive the VEX statements. They cannot.

waybill resolves a closure member's declaration against **plain nixpkgs at
the revision `flake.lock` pins** — not against the project's package set.
A package defined in the project's own flake is not in nixpkgs, so it
resolves to `NoAttribute` and no declaration is ever read.

That is the design working, not a gap in it:

- `meta.knownVulnerabilities` is a statement by *nixpkgs maintainers* about a
  *nixpkgs package*. A locally-defined package has no such statement, and
  inventing one would be fabrication.
- Reaching the project's own package set needs `getFlake` on a path, which
  needs `--impure`, which milestone 1034's argv guard refuses because it
  restores access to the host environment.

**The same applies to overlaid packages.** A project that overrides a nixpkgs
package gets a different output path, so the verification rejects the
candidate and records the member unchecked. Safe — no claim is attached to
the wrong build — but it is why coverage is not 100% and why part of the
measured 8% path-mismatch is overlays rather than name collisions.

## Where the declaration behaviour is tested instead

- **Statement and annotation shape** — unit tests over a hand-built
  `NixpkgsSecuritySummary`, the same way milestone 1035 tests its backport
  statements. No `nix` required, and the shapes are what the spec constrains.
- **Coverage against real data** — the env-gated cross-check in
  `declarations/mod.rs`, run against a real closure and its project.
- **This fixture** — that a scan completes, degrades with a reason when the
  declaration pass cannot run, and emits nothing when `--nix-closure` is
  absent.
