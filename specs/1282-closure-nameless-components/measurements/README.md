# Measurements: the pname-less closure population

Run against the pinned `nix-closure-moat` closure (x86_64-linux, 1,749
derivations). `classify-nameless.py` is the probe; `results.txt` is its output
at the time of writing.

```sh
python3 -I classify-nameless.py <closure.json> --identity
```

Produce the input with `nix derivation show --recursive <flakeref>`.

## What it found

| | |
| --- | --- |
| closure derivations | 1,749 |
| with `pname` (emitted today) | 604 |
| **without `pname` (emitted today: nothing)** | **1,145 — 65%** |

Four classes among the 1,145:

| Class | Count | What they are |
| --- | --- | --- |
| `source` | 549 | fixed-output fetches; 138 carry a URL, 36 of those forge-hosted |
| `package` | 83 | version-bearing bootstrap packages (`mes-libc-mini-0.27.1`) |
| `scaffolding` | 267 | `writeText` output — `itoa-builder`, `setup-hook.sh` |
| `fragment` | 246 | live-bootstrap C functions — `strncpy`, `memcpy`, `malloc` |

## Identity

| Scheme | Distinct | Collisions |
| --- | --- | --- |
| name alone | 957 | **188 collide** |
| name + `.drv` hash | **1,145** | **0** |
| name + content hash (the 549) | 502 | 47, and they are correct merges |

`source` alone covers **56 distinct contents**, which is why a name-keyed
identity is not an option. The 47 "collisions" on the content-hashed path are
the same content fetched by two derivations, which is one component and not
two — so that path is a deduplicator, not a conflict.

Every one of the 1,145 can therefore be told apart: the `.drv` hash is nix's
own content-address of the build recipe, and it is present for every node
including the ones carrying no content hash of their own.

## Three probe errors, recorded because each produced a figure that looked right

1. **`builder` is a top-level key, not an `env` key.** Testing `env["builder"]`
   invented a class of 421 "derivations with no builder at all" that does not
   exist.
2. **A fixed-output derivation records its hash in `outputs[<name>]["hash"]`.**
   `env["outputHash"]` is the older spelling and appears on 139 of the 549 —
   reading only that undercounts the content-addressed population by a factor
   of four.
3. **The top-level `version` key is the derivation *format* version**, `"4"`
   for all 1,749 nodes. Read as a package version it makes every bootstrap
   fragment look versioned, including `strncpy`.

A fourth belief did not survive either: the forge-coordinate path, carried
into this work as the primary identity scheme, covers **36 of 1,145** — 3%.

## Not measured here

**aarch64-darwin.** Every figure above is x86_64-linux. The darwin closure is
a different closure, not a scaling of this one, and milestone 1280 made the
cross-system extrapolation error three times. Any darwin figure this feature
needs must come from a darwin capture.
