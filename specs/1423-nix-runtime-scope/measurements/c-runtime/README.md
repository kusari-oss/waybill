# #1467: C runtime libraries under the reachability walk

Collected 2026-11-11 on aarch64-darwin, scanning the **x86_64-linux**
attribute of the milestone-1377 wrapper flakes
(`specs/1377-nix-subject-identity/measurements/wrapper-flakes/`, nixpkgs
`a799d3e3`). Evaluation is cross-platform and nothing is built.

Each target is scanned twice by the waybill binary itself (`runpair.py`):

- **truth**: default substituters. No local store holds a Linux output, so
  cache.nixos.org answers and its answer is signature-verified (C216
  `method: runtime-closure`, `store: https://cache.nixos.org/`).
- **walk**: `NIX_CONFIG="substituters ="`. No store answers, so the
  reachability walk decides (C216 `method: reachability-walk`,
  `unavailable-reason: not-in-any-store`).

`scopediff.py` compares the CycloneDX `scope` of every closure member
(CDX components walked recursively, keyed by PURL). "main" is waybill at
`61f6a89f`; "fixed" is this change. Full output: `results.txt`.

## Before (main)

The walk marked glibc not-in-artifact on all five; the runtime closure holds
it on all five. Other C runtime libraries it marked that the runtime closure
holds:

| target | glibc | gcc (`lib` output) | xgcc (glibc's libgcc) | libidn2 + libunistring (glibc's) |
|---|---|---|---|---|
| curl | marked | marked | marked | reached by the walk |
| openssh | marked | marked | marked | marked |
| httpie | marked | marked | marked | marked |
| gh | marked | runtime closure lacks it | marked | marked |
| ripgrep | marked | marked | marked | marked |

## Why only glibc is fixed

No dependency field names any of them. glibc reaches a member through
`stdenv` → `stdenv-linux` (`defaultNativeBuildInputs` holds
`gcc-wrapper-15.2.0`) → the wrapper's `libc_lib` = `glibc-2.42-61`. That
chain says which libc every dynamically linked output of the member loads,
so the walk now follows it.

The rest have no such chain:

- gcc's `lib` output (`libgcc_s`, `libstdc++`) is the wrapper's `cc`, the
  compiler itself. Whether an output links it depends on the language (gh, a Go
  binary, does not), which the derivation graph does not record.
- glibc's own references: `xgcc-15.2.0-libgcc` appears only in glibc's
  `makeFlags` (`user-defined-trusted-dirs=…`), and `libidn2` only in
  `postPatch`, as a store path patched into `LIBIDN2_SONAME`. Among
  dependency data it is just one of glibc's `inputs.drvs`, beside build
  tools (bison, gettext, python3-minimal, binutils, bash-static, a bootstrap
  glibc). Reading either would mean treating script text or every input as
  linked.

C216 `c-runtime-libraries: libc-only` says so in the document.

## The bootstrap libc

The first version kept every libc a walked member's compiler named. On curl
it added a false runtime member: `libunistring-1.4.2` is reused from
nixpkgs' bootstrap stage 2, built by `bootstrap-stage2-stdenv-linux`, whose
compiler's `libc_lib` is `bootstrap-stage0-glibc-minimal-bootstrap`. The
truth does not hold it. That bootstrap libc is a direct input of
`glibc-2.42-61`, the libc the other members name, so a libc another found
libc was built from is now dropped.

## After (fixed)

| target | walk vs truth, main: TP / false-excluded / false-runtime | fixed |
|---|---|---|
| curl | 16 / 3 / 7 | 17 / 2 / 7 |
| openssh | 16 / 6 / 45 | 17 / 5 / 45 |
| httpie | 30 / 7 / 3 | 31 / 6 / 3 |
| gh | 1 / 8 / 0 | 2 / 7 / 0 |
| ripgrep | 2 / 5 / 0 | 3 / 4 / 0 |

On each target the only member whose mark changed between the two walks is
`glibc 2.42-61`, and the runtime-closure (truth) runs are identical between
main and fixed.

## Not measured

aarch64-darwin. The same chain ends at `libSystem-B` there (the darwin
`clang-wrapper` carries `libc_lib`), so the rule should apply; that is
inference. A darwin walk could not be forced on this host: the local store
already holds the darwin outputs, so the "walk" run was answered by the
local store and reported `runtime-closure`.

## Reproduce

```sh
python3 -I runpair.py <waybill> <label> x86_64-linux <wrapper-flakes-dir> <out-dir> curl openssh httpie gh ripgrep
python3 -I scopediff.py <out-dir>/<label>/x86_64-linux/curl.rt/cdx.json <out-dir>/<label>/x86_64-linux/curl.walk/cdx.json
```
