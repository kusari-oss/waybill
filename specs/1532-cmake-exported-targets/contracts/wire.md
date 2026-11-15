# Wire contract: CMake exported targets

## Components

No component is added, removed or renamed. A #1531 package component
(`pkg:generic/<name>@<version>`, tier `deployed`) gains evidence entries:
- **CycloneDX:** `evidence.occurrences[]`, one per attributed file. `location`
  is the scan-root-relative path; `sha256` is reused from the binary scan
  where it computed one, and otherwise hashed, subject to the 256 MB cap.
- **All three formats:** `waybill:source-files` (C18) entries.

Binary file components are byte-identical with or without this feature.

## C225 `waybill:installed-library-coverage`

**Gains a key.** `exported-targets` is added, only when at least one
exported-targets file was matched:

```json
{"cmake-version":{…},"exported-targets":{"attached":N,"locations":N,"no-package-version":N,"not-shipped":N,"owned-by-os-package":N,"read":N,"unresolved-location":N},"pkg-config":{…},"skipped":{…},"unresolved-requires":N,"version-disagreements":N}
```

Keys are sorted. With no exported-targets file, C225 is unchanged from #1531.

**Counts:**
- **`read`**: exported-targets files read, meaning recognised locations not
  owned by an OS package.
- **`owned-by-os-package`**: exported-targets files skipped as OS-owned.
- **The rest:** (directory, resolved location) pairs, so a file two packages name counts once for each.
  - `attached`: in some component's evidence.
  - `not-shipped`: resolves under the root, but no file exists there.
  - `unresolved-location`: another variable, no main file, or it escapes
    the root.
  - `no-package-version`: the owning package has no versioned component.
  - `locations`: their total.

## Catalog

C225's row text gains the `exported-targets` object. C18 and the CycloneDX
occurrence shapes are unchanged.

## Invariants

- **No exported-targets file:** the output is identical to the base
  commit's.
- **Nothing is evaluated:** a location with any variable other than
  `${_IMPORT_PREFIX}` is never resolved.
- **Repeated scans** are byte-identical.
