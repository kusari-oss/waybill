# Research

Actual pnpm 9.15.9 produced lockfileVersion 9.0, importer dependencies/devDependencies, and snapshots, with no per-package dev:true. Same manifest through npm 10.9.3 correctly marks four development-only packages and excludes them. Base 2f9be2e625ea20be6da7428327ab7cb3d403e0a8 retains all nine with pnpm.

The reader never reads importers and stamps every unclassified entry Runtime. The existing propagation pass is additive and never changes assigned scope, so direct seeds alone cannot classify transitives. Root discovery works and is not the cause.

Use lock-resolved versions rather than package.json constraints. Follow runtime/optional roots before assigning dev seeds, since a dev direct dependency may also lie inside a runtime subtree. Use existing snapshot identity semantics, including merged peer contexts, rather than introducing a new graph model. Missing importers do not establish scope evidence.

Hpack was compared and deferred: its omission is real, but existing graph completeness explicitly describes emitted reachability, and CDX already marks the target incomplete. It is not part of this patch.

Independent review caught a propagation interaction in candidate v1: removing a protected identity from the direct development set is insufficient if the child manifest is absent. The global pass can still reach the unscoped package from a dev seed and demote it. A freshly pnpm-generated workspace reproduced zero dependencies after filtering, where five must remain. Candidate v2 preserves explicit Runtime evidence for that closure; a full CLI regression scans with and without the child manifest in all formats. This supersedes candidate v1's parser-only protection claim.

Candidate v2 protected runtime closure but not the optional closure from the same downstream propagation interaction. A second authentic pnpm 9.15.9 workspace places string-width under the child importer's optionalDependencies, overlapping the root's dev wrap-ansi closure. Its packages and snapshots contain no optional marker because the dependency is also reached through a non-optional dev edge. Candidate v3 preserves the prior Runtime default for this non-development evidence, after existing optional/peer classification. This prevents demotion without introducing a new Optional classifier. The generated fixture is replayed through the actual CLI with the child manifest present and absent in all three formats.

Fixture provenance: each pnpm fixture was generated from its saved package.json with `corepack pnpm@9.15.9 install --lockfile-only --ignore-scripts`. Workspace fixtures also contain the saved pnpm-workspace.yaml and child package.json used during generation. Logs and generation directories are retained outside the patch in reproduction/. No dependency scripts were executed.
