# Wire contract: hpack `!include` and YAML merge keys

## Documents

- **Components and edges:** a manifest that becomes readable, or whose merged keys now apply, gains its package's main module and dependency edges through the existing Haskell emission. Nothing new is emitted.
- **C200 (`waybill:hpack-read-outcome`):** gains `includes-resolved` and `includes-unresolved` when non-empty (data-model). That is the same value in CycloneDX, SPDX 2.3 and SPDX 3, compared whole by the existing C200 extractors.
- **Unchanged:** a scan with no `!include` and no `<<` (FR-007).

## Fixture cases (`waybill-cli/tests/hpack_include_merge.rs`)

Each is a synthetic `waybill-fixture-*` tree, scanned in three formats with parity.

| Case | Tree | Expected |
|---|---|---|
| M1 | `library: {<<: *a}`, `*a` = `dependencies: [base, containers]` | `base`, `containers`; `read` |
| M2 | two `<<` keys in one mapping | `read`; both mappings' keys apply |
| M3 | two merges both setting `dependencies` (`[base]`, `[text]`) | `base` (the first merge) |
| M4 | merge `[base]` and explicit `[text]`, explicit written first, and again with it last | `text` both times |
| M5 | `<<: *s`, `*s` a sequence | no dependency from it; `read` |
| I1 | manifest includes `common.yaml`, which defines `&deps`; `library: {dependencies: *deps}` | the anchored dependencies; `read`; `includes-resolved` lists it |
| I2 | `<<: !include "a.yaml"`, where `a.yaml` does `<<: !include "b.yaml"` relative to its own directory | `b`'s keys apply; `read` |
| I3 | `library: !include "lib.yaml"`, and `lib.yaml` uses `*d` defined in the manifest before the include | resolved (downward anchor) |
| I4 | an anchor defined in the manifest and redefined in an include after it | the later definition |
| U1 | include of a missing file | `partial`; `includes-unresolved` `missing` |
| U2 | include of `../../outside.yaml`, above the scan root | `partial`; `outside-root`; the file is not read |
| U3 | `a.yaml` and `b.yaml` include each other | `partial`; `cycle`; terminates |
| U4 | a chain of 9 includes | `partial`; `too-deep` |
| U5 | `{deps: !include "x"}` in flow syntax | `partial`; `unsupported-position` |
| N1 | a manifest with neither construct | byte-identical to `main` (the control against T001's binary) |

## Unit tests (`hpack.rs`)

- **The rename (R4):**
  - non-`<<` lines are unchanged;
  - `<<:` inside a block scalar or a quoted string is not renamed;
  - reversing the rename restores the input.
- **The merge fold (R3):** precedence; sequence form; a non-mapping source; a nested merge inside a merged mapping.
- **serde_yaml (R8):** a redefined anchor takes the later definition; a renamed key survives parsing.
