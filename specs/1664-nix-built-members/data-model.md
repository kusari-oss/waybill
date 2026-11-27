# Data model

- **NixBuildRef**: `nix_file: PathBuf`, `line: usize`, `literal: String`, `dir: PathBuf`. The last is the resolved package directory, inside the root. It exists only for `callCabal2nix` with a literal `./` or `../` path, outside a comment.
- **Membership source**: a closed set, `cabal-project | stack-yaml | nix-callcabal2nix`. Only `nix-callcabal2nix` is emitted (C248), and only when neither of the other two admitted the directory.
- **Nix-admitted member**: a manifest directory → the sorted set of `nix_file`s. It feeds C18 (`extra_source_paths`) and C248.

Invariants:
- Nix admission only adds members; it never removes one (FR-005).
- A directory with no `.cabal` or `package.yaml` found by the walk is never admitted.
- A directory outside the scan root is never admitted.
