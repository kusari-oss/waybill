# Data model

No new types are emitted. These are the in-process relationships.

- **Lock**: a freeze file or `stack.yaml.lock`. It has a directory and a set of exact pin names (lowercased).
- **Governed dirs**: `lock_governed_dirs(lock_dir, members)`. That is the lock's directory plus its members, canonical and unique.
  - A freeze file's members are `cabal_project_member_dirs` of the sibling `cabal.project`, or none if there is no sibling.
  - A Stack lock's members are the sibling `stack.yaml` `packages:`, as today.
- **`pins_by_dir`**: maps a canonical directory to pinned names, unioned across locks (unchanged). Phase G reads it by a package's canonical directory.

**Invariant**: a package directory is governed by a lock only if it is the lock's directory or a listed member of the lock's project file.
