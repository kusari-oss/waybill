# Scope Contract

1. Canonicalize importer resolved values through the existing alias parser and pnpm key normalization. Registry package identity is name plus version; link/workspace references do not become invented registry identities.
2. Union runtime and optional roots across importers. BFS over existing snapshot adjacency, preserving version disambiguation and termination on cycles.
3. An identity reachable from runtime or optional roots cannot become Development even if directly declared dev elsewhere.
4. Retain existing optional/peer classification, otherwise stamp explicit Runtime on the protected closure even when the importer manifest is absent. Set only direct development seeds; classify remaining unassigned transitives through existing propagation so C62 records the derivation.
5. Existing optional/peer markers retain semantics; v6/v7 and missing-importer fixtures retain previous behavior.
6. No new CLI flag, field, output format or Yarn behavior.
