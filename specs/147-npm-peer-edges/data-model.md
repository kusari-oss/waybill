# Data Model — milestone 147

Phase 1 output. Defines the new annotation shape + the pre/post behavior table for the npm reader.

## New annotation

### `mikebom:peer-edge-targets`

| Field | Value |
|---|---|
| Key in `PackageDbEntry.extra_annotations` | `"mikebom:peer-edge-targets"` |
| Value type | `serde_json::Value::Array(Vec<serde_json::Value::String>)` |
| Element shape | PURL string (e.g., `"pkg:npm/react-native@0.85.3"`) |
| Element ordering | Alphabetical (lex-ascending), enforced via `BTreeSet<String>` collection — milestone-145 `mikebom:file-paths` precedent |
| Emission gate | OMITTED entirely when the peer-edge-targets set would be empty (zero peers, all peers unmet, or all peers also in regular sections) |
| Wire surface | CDX 1.6 `components[].properties[]` (auto-stringified to `xs:string` via existing `cyclonedx/builder.rs:1086-1098` iteration); SPDX 2.3 envelope annotation (via `spdx/annotations.rs:371` iteration); SPDX 3 envelope annotation (via `spdx/v3_annotations.rs:332` iteration) |
| Cross-format invariance | All three formats carry byte-equivalent array values for the same component (Principle V parity-bridging; covered by SC-002 parity catalog row addition) |

## Modified function

### `parse_package_lock` (in `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:21`)

**Public API surface**: UNCHANGED. The function signature, parameters, return type, and downstream consumer contract are all preserved.

**Behavioral change**: per-`PackageDbEntry` construction at lines ~166-225 now ALSO walks `peerDependencies` (after the existing three sections) and conditionally stamps the new annotation. See research §B for the algorithm sketch.

## Pre/post behavior table

| Lockfile shape | Pre-147 `entry.depends` | Pre-147 `extra_annotations["mikebom:peer-edge-targets"]` | Post-147 `entry.depends` | Post-147 annotation |
|---|---|---|---|---|
| `dependencies: {foo: ^1}` (foo installed) | `["foo@1.0.0"]` | absent | `["foo@1.0.0"]` | absent |
| `peerDependencies: {foo: ^1}` (foo installed via root) | `[]` | absent | `["foo@1.0.0"]` | `Value::Array(["pkg:npm/foo@1.0.0"])` |
| `peerDependencies: {foo: ^1}` (foo NOT in lockfile) | `[]` | absent | `[]` | absent (FR-002 unmet-peer gate) |
| `dependencies: {foo: ^1}, peerDependencies: {foo: ^1}` (same name both) | `["foo@1.0.0"]` | absent | `["foo@1.0.0"]` | absent (FR-003 regular wins) |
| `peerDependencies: {foo: ^1, bar: ^2}` (both installed) | `[]` | absent | `["bar@2.0.0", "foo@1.0.0"]` (BTreeMap key-sorted) | `Value::Array(["pkg:npm/bar@2.0.0", "pkg:npm/foo@1.0.0"])` (alphabetical) |
| `peerDependencies: {foo: ^1}` + `optionalDependencies: {foo: ^1}` | `[]` (peer skipped) OR `["foo@1.0.0"]` (optional kept depending on path) | absent | `["foo@1.0.0"]` | absent (FR-003 — optional is regular) |

## Validation rules (consolidated from spec FRs)

| Input | Rule | Source |
|---|---|---|
| Section iteration order | `dependencies` > `devDependencies` > `optionalDependencies` > `peerDependencies` | FR-001 + FR-003 |
| Peer name already in `depends_set` from a regular section | Skip — no peer-edge-target entry; no double-emission | FR-003 |
| Peer resolved via `resolve_dep_via_node_modules_walk` to `Some(version)` | Emit edge + add PURL to peer-edge-targets BTreeSet | FR-001 + FR-004 |
| Peer resolved to `None` (unmet) | NO edge, NO peer-edge-targets entry | FR-002 |
| `peer_edge_targets.is_empty()` after iteration | OMIT the annotation key entirely | FR-005 |
| Annotation array element shape | PURL string per `mikebom_common::types::purl::Purl::new()` format | Existing reader convention |
| Annotation array order | Alphabetical (lex-ascending) via `BTreeSet` iteration | research §A + milestone-145 precedent |

## Out of model

- **No new types** (no structs / enums / newtypes introduced).
- **No public API changes**: `parse_package_lock` signature unchanged.
- **No call-site changes elsewhere**: emitters consume `extra_annotations` transparently via the existing milestone-127 `is_internal_emission_key` iteration pattern.
- **No changes to v1/v2 lockfile walker** (different code path; out of v1.0 scope per spec).
- **No changes to yarn/pnpm/bun readers** (out of scope per spec).
