# T004: agreement with hpack on `main` (the control)

`hpack_agreement.py` was run against the binary at `ea149065` on each no-generated-cabal copy (`corpus-copies.md`), with hpack 0.38.3. hpack ran in a throwaway third copy of each repository. Afterwards, the four no-generated-cabal copies still had **0** hpack-generated `.cabal`.

| Repository | hpack packages | agree | differ | of which missing from waybill |
|---|---|---|---|---|
| hs-opentelemetry | 24 | 8 | 16 | 16 |
| lootbox | 7 | 1 | 6 | 6 |
| hs-temporal-sdk | 7 | 7 | 0 | 0 |
| ogmios | 8 | 0 | 8 | 8 |

- **Every difference on `main` is a missing package**, one per `package.yaml` waybill leaves unread: 30 in all, matching C200. No package waybill reads has a dependency set different from hpack's, so the check can fail, and fails exactly where #1673 and #1674 bite.
- **hs-temporal-sdk already agrees fully.** Its 6 `partial` manifests' includes carry no dependencies.

## How the comparison was refined (as built)

The first run differed on 35 of 37 hs-opentelemetry packages, for reasons outside this spec:

- **A package's own name.** hpack lists it in `build-depends`, because its executables and tests depend on its library, and waybill emits no edge from a package to itself. The comparison now drops it.
- **Build tools.** hpack renders `build-tools` as `build-tool-depends`, which the first version did not read. It now does, keeping the package part of `pkg:exe`.
- **A qualified build tool.** waybill keeps `hspec-discover:hspec-discover` whole as a package name. That is a separate defect, filed as https://github.com/kusari-oss/waybill/issues/1684; the comparison counts it apart and compares the package part.
- **Packages with a hand-written `.cabal` and no `package.yaml`** (13 in hs-opentelemetry) have no hpack side. They are listed, not compared.
