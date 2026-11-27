# Data model

- **StackLockEntry** gains `source: PathBuf`, the `stack.yaml.lock` it came from.
- **CabalFreezeEntry::ExactPin** gains `source: PathBuf`, the `cabal.project.freeze` it came from.
- **Lock-pinned component**: `source_path` is the first file that pinned its PURL, and `extra_source_paths` holds every later one, sorted and unique. Together they make up C18.
- **Invariant:** a file appears on a component only if it pinned exactly that PURL (name and version).
