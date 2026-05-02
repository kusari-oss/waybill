# Fixture: argo-style-no-cache

Tarball-style trimmed Go project mirroring the shape of
`github.com/argoproj/argo-workflows v3.3.9` (the issue #102
reproduction case).

## Purpose

Locks SC-001 + SC-007 for milestone 053:

- ≥ 14 `DEPENDS_ON` edges from the synthetic main-module to direct
  requires, even when GOMODCACHE is empty (the dominant
  fresh-clone CI workflow).
- Cross-host byte-identity: NO `.git` directory, so the version-
  resolution ladder always falls to step 3 (`v0.0.0-unknown`).
  Goldens are deterministic across linux + macOS.

## Shape

- Trimmed `go.mod` declaring 14 direct requires (mirrors argo's
  shape; we don't ship the actual content, just the structure).
- No `go.sum` (the issue #102 case is "no module cache, no sum
  file content needed for the fix" — direct edges come from go.mod
  alone).
- No `.git` directory (forces step 3 of the version ladder).

## Why a fixture rather than the live argo-workflows clone?

CI runners can't depend on network access for `git clone`, and
argo-workflows v3.3.9's `go.mod` shape may evolve over time
(argo's authors could rebase the tag, add/remove requires, etc.).
The fixture is a deterministic snapshot that locks the
reproduction case at a fixed moment.
