# Corpus on `main` 68dc440c (T002)

- 24 C45 `edgeless-lock-pin` components: 1 in each hs-opentelemetry variant and 11 in each lootbox variant, which is 12 distinct PURLs. Every one lists `stack.yaml.lock` as its source file, and none comes from a freeze file.
- 4 goldens name `cabal.project.freeze`: haskell-aeson, hpack-hs-temporal-sdk, hpack-ogmios, and hpack-ogmios-no-generated-cabal. In each, 0 versionless `pkg:hackage` components share a name with a freeze-pinned component.
- **Prediction:** no corpus golden changes. The fixture is the evidence (spec assumption).
