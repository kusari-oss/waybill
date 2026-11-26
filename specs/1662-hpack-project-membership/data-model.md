# Data model: spec 1662

- **Stand-in path**: `D/<name>.cabal`, standing for a `package.yaml` at `D/package.yaml`. `<name>` is the manifest's `name:`, or `D`'s own name if absent or unreadable. It is used only for membership matching and never emitted.
- **Membership verdict**: for each candidate (a real `.cabal` or a stand-in) inside a governing `cabal.project`'s tree, declared or not. "Nothing declared" across both kinds means the tree is left alone.
- **Measurement row** (`measurements/membership.tsv`): target, pin, variant, waybill revision (`main` or branch), sorted `pkg:hackage` set, and the reason for each difference.
