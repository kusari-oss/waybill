# Data Model: npm-ecosystem optional-axis classification

**Milestone**: 1272 | **Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

No new persisted types and no schema change. The emitted representation
of the optional axis is already established — `LifecycleScope::Optional`
on the component, `RelationshipType::OptionalDependsOn` on its incoming
edges, CDX `scope: "excluded"`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`, SPDX
3.0.1 annotation-only (catalog row C122). This feature changes which
components receive that classification, not how it is expressed.

What follows is the in-process state each reader builds during one
parse, and dies with it.

## Entities

### OptionalSeed

The set of package identities a lockfile or manifest declares optional
*directly*. One per reader, built from that dialect's own location.

| field | type | source per dialect |
|---|---|---|
| identities | set of identity | pnpm: n/a (closure already published); bun: `workspaces.<p>.optionalDependencies` keys; Berry: lockfile entry `dependenciesMeta.<n>.optional == true` ∪ manifest `dependenciesMeta.<n>.optional == true`; yarn v1: manifest `optionalDependencies` keys ∪ per-entry `optionalDependencies:` sub-block names |

Validation:
- An identity with no corresponding lockfile entry is dropped with a
  diagnostic and never fabricated (FR-012).
- A `link:` / `file:` / `workspace:` target invents no registry
  identity, matching the development axis (spec Edge Cases).

### RuntimeSeed

The set of identities declared under a non-optional, non-development
section. The seed of the cancelling closure.

Validation:
- **MUST NOT include the optional declarations.** The existing
  development-axis helper `v9_importer_scopes` deliberately merges
  `dependencies` and `optionalDependencies` into one `protected` set
  because for that axis the distinction is irrelevant. Reusing it
  unsplit here would make the cancelling set contain the classification
  subjects, and FR-006 would subtract a set from itself. See research
  R7.

### RuntimeClosure

`RuntimeSeed` closed transitively over the reader's edge map — every
identity reachable from a non-optional declaration by any edge.

This is the cancelling set. Measured against both reference
implementations in Probe C: a package inside it is not optional even
when a root declared it optional (FR-006).

### OptionalClosure

`OptionalSeed` closed transitively over the reader's edge map, then
`RuntimeClosure` subtracted.

Measured in Probe C's control case: an identity in this set but in no
declaration at all is still optional in both reference implementations
(FR-007a).

Final classification per identity:

```
optional  ⟺  identity ∈ OptionalClosure
              ∧ identity ∉ RuntimeClosure
              ∧ ¬is_peer_optional(identity)      // FR-009
              ∧ ¬is_development(identity)         // FR-010 precedence
```

For pnpm v9 this whole structure collapses: the lockfile publishes
`snapshots.<id>.optional` already closed and already cancelled, so the
reader reads one boolean and builds none of the above (research R3).

### Edge map (existing, per reader — not introduced here)

| reader | key shape | where it already lives |
|---|---|---|
| pnpm v9 | `name@version` | `build_snapshots_lookup` → `HashMap<String, Vec<String>>` (`pnpm_lock.rs:141`) |
| bun | packages key-path | `metadata` tuple position `[2]`, walked at `bun_lock.rs:690-806` |
| yarn Berry | first descriptor per entry | the `mapping` loop at `yarn_lock.rs:139+` |
| yarn v1 | bare name | the line-format walk at `yarn_lock.rs:264-320` |

The three key shapes differ, which is why no shared closure helper is
introduced in this milestone (research R7).

### LockfileFormatVersion

Read per dialect purely to decide whether to warn (FR-011a / SC-008).

| dialect | marker | known value |
|---|---|---|
| pnpm | `lockfileVersion` | `6.0`, `7.x`, `9.0` |
| bun | `lockfileVersion` | `2` |
| Berry | `__metadata.version` | `8` |
| yarn v1 | `# yarn lockfile v1` header | v1 |

State transition: an unrecognized higher value does **not** change
parsing. The reader attempts every location it knows, emits a
diagnostic naming the version, and reports no axis rather than an empty
one. It never fails the scan.

### Derivation annotation (existing vocabulary — extended, not redefined)

`waybill:optional-derivation`, catalog row C122. Values already in the
tree: `optional-dependencies` (10), `npm-optional-dependencies` (10),
`pip-optional-dependencies` (8), `bun-optional-dependencies` (5),
`bun-optional-peers` (3).

Every newly optional-classified component carries one (FR-008). The
pnpm v9 arm reuses `npm-optional-dependencies`, which its own
classifier already inserts; the bun arm reuses
`bun-optional-dependencies`. Any value this feature adds must be
accompanied by its parity extractor, or
`parity::extractors::tests::every_catalog_row_has_an_extractor` fails —
see `feedback_sbom_format_mapping_extractor_gate`.

## Relationships

```
manifest / lockfile
      │
      ├── OptionalSeed ──┐
      │                  ├── close over edge map ──► OptionalClosure
      └── RuntimeSeed ───┤                                  │
                         └── close over edge map ──► RuntimeClosure
                                                            │
                                            subtract ◄──────┘
                                                            │
                        peer-optional guard (FR-009) ◄───────┤
                        development precedence (FR-010) ◄────┤
                                                            ▼
                                        LifecycleScope::Optional
                                      + waybill:optional-derivation
                                                            │
                        existing m179 emission machinery ◄───┘
                     (OptionalDependsOn on each incoming edge)
```

Nothing downstream of `LifecycleScope::Optional` changes.
