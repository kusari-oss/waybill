# Contract: Haskell project membership (#1032, specs 1662 and 1664)

A `.cabal` or `package.yaml` under a governing `cabal.project` (one that declares `packages:` matching at least one manifest) is a member iff its directory is:

1. matched by the `cabal.project`'s `packages:` entries; or
2. listed in a sibling `stack.yaml`'s `packages:`; or
3. **(spec 1664)** the resolved directory of a `callCabal2nix "<name>" <path>` in some `*.nix` file in the scan root, where `<path>` is an unquoted literal starting `./` or `../` with no `${`, resolved against the containing file's directory, outside any comment.

Emission for case 3, when cases 1 and 2 do not hold:
- `waybill:source-files` (C18) includes each admitting `.nix` file (rootfs-relative, sorted);
- `waybill:membership-source` (C248) = `"nix-callcabal2nix"`, a per-component property in CDX and an annotation (`MikebomAnnotationCommentV1`) in SPDX 2.3 and 3.

Not recognised (explicitly): `callCabal2nixWithOptions`, `callPackage`, `packageSourceOverrides`, haskell.nix, interpolated or quoted paths, variables, `fetchTarball`/`fetchGit` sources, absolute paths, and any path outside the scan root.

No Nix evaluation and no subprocess. `.nix` files are opened only when rule 1 ∪ 2 would drop a manifest.
