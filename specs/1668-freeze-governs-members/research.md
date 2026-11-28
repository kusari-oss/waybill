# Research: A cabal.project.freeze governs the packages its cabal.project lists

These findings come from reading code at `main` 68dc440c. The behaviour itself is measured at T001.

## R1. Where governance is decided

- **Freeze files, Phase B (`haskell.rs` ~1069):** pins go into `pins_by_dir[canonical(freeze dir)]` only.
- **Stack locks, Phase C (~1113):** `governed` is the lock's directory plus each `stack.yaml` `packages:` entry, joined to it. Pins go into `pins_by_dir[canonical(g)]` for each `g` (spec 1663).
- **Lookup, Phase G (~1380):** `pins_by_dir.get(canonical(cabal_path.parent()))`. An hpack package is looked up the same way through its stand-in `.cabal` path, which is to be confirmed by the hpack variant of the fixture.
- **Consequence:** a member in `app/` has no entry, so its dependency falls back to design tier without a version, and the freeze pin is emitted unreachable (Phase F).

## R2. One helper

- **Decision**: `fn lock_governed_dirs(lock_dir: &Path, members: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf>`. It returns `lock_dir` plus the members, each canonicalised and with duplicates removed. Phase B and Phase C each pass their own member source.
- **Rationale**: the shared step is "own directory plus listed members, canonical"; how members are read differs by lock kind. That keeps FR-003 (one helper) without forcing `stack.yaml` reading into freeze handling or the reverse.
- **Rejected alternative**: a separate copy in Phase B, which is how #1744 arose.

## R3. Reading `packages:` once

- **Decision**: take the per-entry expansion out of `resolve_root_declared_members` into `fn cabal_project_member_dirs(text: &str, candidates: &BTreeSet<String>) -> BTreeSet<String>`. It handles comma and whitespace splitting, `./`, trailing `/`, `.cabal` naming its directory, `literal_separator` globs, and `.`.
  - `resolve_root_declared_members` calls it for the root project.
  - Phase B calls it for the `cabal.project` beside the freeze file, with keys relative to that directory.
- **Candidates**: the discovered package directories (`.cabal` and `package.yaml` parents), keyed relative to the project directory. A directory outside the project directory has no key and cannot match. This covers the spec's `../x` edge case.
- **Check**: the existing `resolve_root_declared_members` unit tests pass unchanged.

## R4. What else could move

- **C45:** `generate/orphan_reason.rs` assigns `edgeless-lock-pin` from the final graph. A pin that becomes reachable loses it with no change to that pass (spec assumption).
- **`waybill:source-files`:** spec 1665 already adds the freeze file to a pinned component. The twin merge leaves one component.
- **Pin union:** a stack lock and a freeze file on one directory are unioned by `pins_by_dir` as today. The existing tests cover it, and this change adds no test for it.
- **Corpus:** unknown until T001. Candidates are targets with a root `cabal.project.freeze` and members in subdirectories. If none exists, the goldens don't change.
