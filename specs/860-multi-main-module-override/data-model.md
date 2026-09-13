# Data model — multi-main-module root override

No new persisted state. Everything below is in-process during a single
emission, matching every milestone since 002.

## Changed structure

### `DropOrDemoteResult` (`generate/root_selector.rs:492`)

| field | before | after |
|-------|--------|-------|
| `effective_components` | main modules removed when override active and N>1, or preserve off | main modules always retained, demoted |
| `redirected_main_module_purls` | PURLs whose outbound edges are removed and re-anchored onto the root | renamed `retained_main_module_purls`; PURLs the root must now depend on |

The field is renamed rather than repurposed silently: its meaning
inverts (from "edges to take away" to "components to point at"), and a
name that survives an inverted meaning is how the next reader gets it
wrong.

## Component transformation

A component holding `waybill:component-role = main-module` under an
active override becomes:

| attribute | transformation |
|-----------|----------------|
| `waybill:component-role` | removed |
| `waybill:demoted-from-main-module` | added, `"true"` (C102, unchanged) |
| emitted type | `application` → `library` |
| PURL, name, version, licenses, hashes | unchanged |
| outbound edges | **retained** (FR-007 — changed from m149) |
| inbound edges | unchanged; they now resolve, which is the defect being fixed |

Identical at every N. There is no separate N=1 path (FR-005).

## Graph shape

```
before (N>1, override active)      after
-----------------------------      -----
root                                root
 ├── (absorbed module edges)         ├── module A ── lib1
 ?                                   ├── module B ── lib2
 (modules absent; inbound            └── module C ── module A
  edges dangle)                          (inter-module edges retained)
```

The root depends on **every** retained module (FR-008), not only those
nothing else depends on — so `module A` above is reachable from the root
directly as well as via `module C`.

## Identity collision (FR-011)

When a retained module's PURL equals the override root's PURL:

- the module is not emitted as a separate component
- its outbound edges attach to the root
- no root→module edge is emitted for it (it *is* the root)

Reachable because waybill mints `pkg:generic/` main modules for some
ecosystems (`scan_cmd.rs:1648`), the same namespace `--root-name` uses.

## Invariants

- **I1**: exactly one subject per document (FR-003).
- **I2**: every dependency reference resolves to a component in the same
  document (FR-001).
- **I3**: every retained module is reachable from the subject (FR-008).
- **I4**: no two components assert the same coordinate (FR-011).
