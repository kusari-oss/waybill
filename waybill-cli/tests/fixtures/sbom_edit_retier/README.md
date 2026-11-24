# `sbom edit --retier` fixture (spec 1651)

`project/` is a synthetic npm tree that `tests/sbom_edit_retier.rs` scans **at run time**, so nothing here is a golden:

- `locked/`: a manifest and lockfile, so its dependency scans as `source`;
- `declared/`: a manifest only, so its dependencies scan as `design`. One of them is a runtime dependency; the other is a dev dependency, `scope: excluded`.

The completeness reason is `transitive-edges-unresolvable: npm`, which case C1 needs.
