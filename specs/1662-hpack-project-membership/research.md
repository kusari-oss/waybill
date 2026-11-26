# Research: spec 1662

## R1. What hpack names its generated `.cabal`

**Measured** with hpack 0.38.3 (`nix run nixpkgs#hpack`) in a scratch directory named `somedir`:
- With `name: waybill-fixture-other`, it generated `waybill-fixture-other.cabal`.
- With no `name:`, it printed `WARNING: Package name not specified, inferred "nonamedir"` and generated `nonamedir.cabal`.

**Decision**: a `package.yaml` in directory `D` stands for the file `D/<name>.cabal`, where `<name>` is the manifest's `name:`, or `D`'s own name when `name:` is absent. That is the file `hpack` would write, so an entry that names it matches.

## R2. How each entry form matches a directory holding only a `package.yaml`

`cabal_files_declared_by` (`haskell.rs`) turns each entry into a glob that never crosses `/`:

| Entry | Glob | `.cabal` in `D` | `package.yaml` in `D` |
|---|---|---|---|
| `D`, `./D`, `D/` | `D/*.cabal` | matches | matches, because the stand-in is `D/<name>.cabal` |
| glob such as `modules/*` | `modules/*/*.cabal` | matches | matches |
| `.`, `./`, empty | `*.cabal` at the project root | matches root only | matches root only |
| `D/foo.cabal` | literal | matches if the file is `foo.cabal` | matches if `<name>` is `foo` |
| `D/*.cabal`, `*/*.cabal` | as written | matches | matches |

**Decision**: feed each `package.yaml`'s stand-in path into the existing matcher next to the real `.cabal` paths. One implementation and one verdict per directory, which satisfies FR-001 and FR-002 by construction.

**Alternatives rejected**:
- **Matching the `package.yaml` directory against entries separately.** That's a second implementation. It gets the explicit `.cabal` form wrong, because that form names a file, not a directory.
- **Reusing spec 1652's `resolve_root_declared_members`.** It only considers files at the scan root and turns entries into directory patterns, dropping the file name, so the explicit `.cabal` form would match any package in the directory. #1032's filter considers every `cabal.project` in the tree, which is the scope this spec needs.

**cabal-install itself** doesn't read `package.yaml`. A directory entry naming a directory with only a `package.yaml` fails `cabal build` until `hpack` runs. Hpack users run it first, which is why the generated-file model is the right reading of intent.

## R3. Where the name comes from

`hpack::read_manifest` already parses the manifest, `!include` included, for the emit loop.

**Decision**:
- Read each manifest once, before the filter, and keep the result for the emit loop, so nothing is parsed twice.
- If a manifest can't be read, the stand-in uses the directory name (R1's fallback).
- That only affects the explicit-`.cabal`-path form, where an unreadable manifest whose `name:` differs from its directory isn't matched by an entry naming `<name>.cabal`. It falls into FR-005's fallback only if nothing else in the project matches. The test set records this case.

## R4. The fallback (FR-005)

Today `declared.is_empty()` is computed over `.cabal` candidates only. With the stand-ins added to the same candidate list, "nothing matched" means nothing matched among either kind. That gives the required behaviour in both directions with no extra code.

## R5. The baseline already exists in the corpus

- `hpack-ogmios` and `hpack-ogmios-no-generated-cabal` are corpus targets (`manifest.rs:809`, `:822`).
- Both goldens contain `pkg:hackage/json-via-show` (4 occurrences in each `cdx.json`).
- None of the other hpack targets, nor haskell-language-server, mention it.

The FR-006 measurement still runs the binaries, on `main` and on the branch, because goldens are masked and may lag. The goldens are the expected point of change: the regenerated ogmios goldens should lose `json-via-show`, and every other target's goldens should be unchanged unless the measurement explains why.

## R6. Generated sibling (FR-003)

In the emit loop, a `package.yaml` suppresses its generated `.cabal` sibling. The stand-in name equals that sibling's name, so both get the same verdict, and FR-003 keeps its current single component. A hand-written `.cabal` beside a `package.yaml` is matched on its own name, as today.

## R7. Union with `stack.yaml` (added at implementation)

**Finding:** T011's first branch measurement dropped two packages that another build definition builds (`measurements/membership.md`).

**Decision (maintainer):** for each `cabal.project`, its entries plus the `packages:` of a `stack.yaml` in the same directory (Stack's default `["."]` when the field is absent). Both readers use it.
- Stack entries are plain directories, which the matcher already handles as directory entries.
- The `stack.yaml` reading is factored into `stack_yaml_packages`, which spec 1652's `resolve_root_declared_members` now shares, so there's no second parser.
- A `stack.yaml` with no `cabal.project` still governs nothing.

**Not covered:** a package built only by Nix (hs-temporal-sdk's `tix-to-markdown`).
