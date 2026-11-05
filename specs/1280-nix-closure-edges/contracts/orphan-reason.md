# Contract: the FR-011 orphan reason code

## Wire value

```
reachable-only-through-unemitted-package
```

One new variant on `OrphanReasonCode`
(`waybill-cli/src/generate/orphan_reason.rs`), carried in the existing
`waybill:orphan-reason` annotation. **The literal string is frozen on landing**
— that enum's own comment says consumers key on these values.

## What it claims

Every path from the root to this component runs through a derivation that
declares a `pname` and that this scan did not emit as a component. The
component **is** depended upon; waybill cannot show by what.

It is therefore distinct from an orphan nothing depends on, which is the
distinction this whole line of work exists to make.

## Why no existing code fits

| code | why not |
|---|---|
| `build-toolchain` | nearest, and wrong: it claims the component is tooling rather than a dependency of the subject. This population is a real dependency with an unshowable path. Reusing it would make one code mean two things — the defect milestone 1279 just spent itself removing from `unreferenced`. |
| `unreferenced-lock-node` | about a lockfile node no other node references. There is no lockfile here, and the component *is* referenced. |
| `dead-lockfile-entry`, `stale-go-sum-entry`, `hoisted-unused`, `unresolved-indirect-require`, `flat-attached-fallback`, `foreign-build-system` | each names a specific ecosystem mechanism; none applies to a nix closure. |

## Populations it MUST NOT appear on

- Any component with no `waybill:closure-role` — it is not a closure component,
  so this cause cannot apply.
- Any component reachable from the root. The code explains an orphan; emitting
  it on a connected component would make the annotation meaningless.
- Any scan without `--nix-closure`.

## Measured population

| system | count | the components | consumed only by |
|---|---|---|---|
| x86_64-linux | **5** | `diffutils 3.12`, `ed 1.22.5`, `gnumake 4.4.1`, `gnutar 1.35`, `pyproject-hooks 1.2.0` | `stdenv-linux`, `patch 2.8`, `build 1.4.4` |
| aarch64-darwin | **16** | `adv_cmds`, `bc`, `bmake`, `bootstrap-stage0-llvm`, `compiler-rt`, `cpio`, `diffutils`, `ed`, `getopt`, `gnumake`, `gnutar`, `lld`, `locale`, `pbzx`, `pyproject-hooks`, `version-check-hook` | `stdenv-darwin`, `clang-wrapper`, `macOS-SDK`, `bmake`, `locale` |

The cause is uniform across all 21 across both systems, which is what justifies
one code rather than a family.

## What closes this population

Emitting the intermediate packages — `stdenv-linux` has no good reason to be
absent from a build SBOM. That is the milestone after next, and when it lands
this code's population should shrink toward zero. It is **not** expected to
reach zero, and the code should not be deleted when it shrinks: a closure can
always contain a package the emission rule declines.
