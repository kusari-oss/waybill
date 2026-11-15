# Data model: CMake exported targets

Everything is in process for one scan.

## TargetsFile (phase 1, one per matched file)

| Field | Notes |
|---|---|
| `path` | as the walker saw it |
| `dir` | `path.parent()` |
| `role` | `Main` (no configuration suffix) or `Config(String)` |
| `raise` | `Option<usize>`: the count of `get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)` lines. Set only for `Main` |
| `locations` | raw `IMPORTED_LOCATION[_<CONFIG>]` strings, in file order |
| `prefix` | the install prefix (CMake search-location rules), or `None`, which counts as unrecognised and is skipped like any #1531 file |
| `os_owned` | set in phase 2 |

## Resolution (phase 2)

For each directory with targets files:

1. **The raise count** is `Main.raise`. Without a `Main` file, every location in the directory is `unresolved-location`.
2. **Each location string:**
   - it must start with `${_IMPORT_PREFIX}/` and contain no other `${`, or it is `unresolved-location`;
   - `_IMPORT_PREFIX` is `dir` raised `raise` times;
   - the file resolves under the scan root, root-relatively (spec 1531 `location::resolve_in_root`). Escaping the root makes it `unresolved-location`, and a missing file makes it `not-shipped`.
3. **The package component**, first match wins (research R4):
   - the CMake-version live record whose directory is `dir`;
   - otherwise, a live record with the same prefix whose `lname` is `dir`'s lowercased name;
   - otherwise none, and the location counts as `no-package-version`.
4. **Attachment.** The resolved location's absolute path is added to that entry's `extra_source_paths`, deduplicated and sorted. It counts once in `attached`.

Locations are deduplicated per directory before counting, across configuration files. Every count is per (directory, resolved location) pair, so a file that two packages name counts once for each (analysis I1).

## ExportedTargetsCoverage (inside C225)

```json
"exported-targets": {"attached": N, "locations": N, "no-package-version": N, "not-shipped": N, "owned-by-os-package": N, "read": N, "unresolved-location": N}
```

- **`read`**: targets files matched in recognised locations and not OS-owned.
- **`owned-by-os-package`**: targets files skipped as OS-owned.
- **The rest** count (directory, resolved location) pairs, per the rule above.

The object is present only when at least one targets file was matched. Otherwise C225 keeps its #1531 shape exactly.

## Hash reuse

`manifest_sha_cache` (`scan_fs/mod.rs:868`) is seeded, before the evidence loop, with `(binary component source_path → file-sha256 qualifier)`. That holds for every binary file component carrying the qualifier.

- **Key form (analysis U2; resolved by measurement).** The cache key is the path string. The analysis feared one file could have two spellings, which would miss the key. Measured: the binary scan reports a file under the same root-relative spelling that `resolve_in_root` produces, even through a directory symlink. So no mapping is needed (`measurements/mutations.md`, M7), and `an_attributed_binary_keeps_the_binary_components_spelling` pins it.
- **Observability (analysis U1).** The post-pass that builds the evidence logs one INFO line, `hash cache: seeded=<n> reused=<m>`. Here `reused` counts lookups answered from a seeded entry.
