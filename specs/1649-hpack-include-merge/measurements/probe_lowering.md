# T018 checkpoint: hpack's lowering, measured (`probe_lowering.sh`, hpack 0.38.3)

Resolving includes made 30 more corpus manifests readable, which exposed three defects in how the hpack reader **lowers** a manifest. They predate this spec (milestone 1273) but were hidden, because the manifests they affect could not be parsed until now.

| Manifest | hpack renders | waybill at `ea149065` |
|---|---|---|
| `when:` written as one mapping, not a list | `if os(linux)` / `build-depends: unix` | `not-a-manifest`: `invalid type: map, expected a sequence` |
| `QuickCheck >= 2`, `quickcheck < 3` | two entries, `QuickCheck >=2` and `quickcheck <3` | both lowercased to `quickcheck`, one entry, `pkg:hackage/quickcheck` |
| `text >= 1`, `text < 3`, `Text` | `Text` and `text <3`: last-wins per exact spelling | one `text` entry |
| `waybill-fixture-x:sub`, `waybill-fixture-x:other` | both, as written (Cabal's `package:library` form) | `pkg:hackage/waybill-fixture-x:sub` and `:other` |
| `build-tools: hspec-discover:hspec-discover` | `build-tool-depends: hspec-discover:hspec-discover` | `pkg:hackage/hspec-discover:hspec-discover` (#1684) |

**hpack's rules:**
- a name keeps its declared spelling;
- last-wins applies per exact spelling;
- `package:qualifier` names the package before the colon.

**The `.cabal` reader already follows all of this.** It has kept the declared spelling since #943, and it splits `package:qualifier` at the colon (`haskell.rs`, `parse_build_depends`).

**On the corpus, the defects are a regression this spec would cause.** Once ogmios's committed manifests are read, they suppress the generated `.cabal` that got these identities right. The branch's first measurement showed:
- committed ogmios: `ouroboros-network` → `ouroboros-network:api`, `:framework` and `:protocols`;
- committed hs-opentelemetry: a duplicate `pkg:hackage/quickcheck` beside `pkg:hackage/QuickCheck`.

**Decision:** fixed in this spec, as FR-009 and FR-010, so that it does not ship a regression. The lowering now matches hpack and the `.cabal` reader.
