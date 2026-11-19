# Data model

No new catalog row and no new value. C42, C62 and C122 move on the affected records only. C236 keeps its values; a required record may now carry it (research R3).

## Pairing (in process)

| Field | Today | After |
|---|---|---|
| `conditional` | the search has any scope | unchanged |
| `conditions` | the search's C236 conditions | unchanged |
| `scope` | none | **new:** the search's `LifecycleScope` (`Optional` or `Test`) |
| `test_derivation` | none | **new:** the search's C62 when it is Test (`source-test-directory`, `cmake-build-testing`) |

## RecordScopes (in process)

| Set | Meaning | Applied as |
|---|---|---|
| `required` | some search is unconditional | unchanged; switches off the link half for the record |
| `optional` | every pairing is conditional, at least one is not Test, the record ships no library file, and nothing else scopes it | Optional, C122 `cmake-conditional`, plus C236 when there are conditions (unchanged) |
| `test` (**new**) | every pairing is Test, the record ships no library file, and nothing else scopes it | Test, with C62 `source-test-directory` if any pairing has it, else `cmake-build-testing`; C236 when there are conditions |
| `conditions_only` (**new**) | today's rule would have made it Optional, but it ships a library file | no scope change; C236 when there are conditions |

## "Ships a library file"

The record's own evidence (`evidence.source_file_paths`, filled by `installed_lib::resolve`) has a path whose file name ends in `.so`, `.so.<N…>`, `.a`, `.dylib`, `.dll` or `.lib`.

## Invariants

- A record that ships a library file is never made Optional or Test by the search half.
- The link half (C235) is evaluated exactly as today.
