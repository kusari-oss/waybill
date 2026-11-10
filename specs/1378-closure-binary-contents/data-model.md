# Data model: closure binary contents

All in-process, per scan. Nothing persists beyond the emitted document.

> **Superseded on rebase (2026-11-10).** While this was being built, main
> landed milestone 1377 (#1405, issues #1398 and #1394): the package a closure
> scan evaluated is always emitted and is the document's subject, identified
> as `graph.evaluated` — the closure's sole unconsumed derivation. That is the
> root-as-component work this feature had taken on as "the minimum of #1398".
> On rebase the feature's own mechanism (a `root` value for
> `waybill:closure-role`, `DerivationRole::Root`, `graph::sole_root`) was
> **removed** in favour of main's. Text below that describes it is history.
> Main also took row C212, so this feature's rows are **C213, C214, C215**.

## `DerivationRole::Root` (extends an existing enum)

`closure/classify.rs`. Wire value `root`. Assigned to the closure root when no
declared attribute or graph referrer gives it another role. Emits on role
alone. Merging two variants of one identity: `Root` wins over `Unreferenced`
and loses to nothing else it can meet (a root is not also a dependency).

## `EmbeddedContents` (new, `scan_fs/binary/embedded.rs`)

What the two readers found under one path.

| field | type | notes |
| --- | --- | --- |
| `modules` | `Vec<PackageDbEntry>` | Go dependency + `stdlib` entries and non-root crates, exactly as the standalone readers build them; `parent_purl` cleared |
| `self_descriptions` | `Vec<SelfDescription>` | one per binary that names itself |
| `binaries_examined` | `usize` | binaries discovered under the path |
| `failed` | `bool` | a discovered binary could not be read |

## `SelfDescription`

| field | type | notes |
| --- | --- | --- |
| `ecosystem` | `SelfEcosystem` (`Golang` \| `Cargo`) | |
| `name` | `String` | Go module path or crate name |
| `version` | `Option<String>` | `None` for Go's `(devel)` or empty |
| `binary` | `PathBuf` | the file it came from |

## `ReadState` (new, `closure/contents.rs`)

`Read` \| `Partial` \| `Absent` \| `Failed` \| `NoOutputs`. Wire: `read`,
`partial`, `absent`, `failed`, `no-outputs`. Precedence when combining a
member's outputs: `Failed` > `Partial` > (`Read` or `Absent` when uniform).

## `BinaryIdentity` (new) — the C215 value

| field | wire key | notes |
| --- | --- | --- |
| candidate PURL | `candidate` | absent when status is `ambiguous` or `no-version` |
| status | `status` | `confirmed` \| `declined` \| `unchecked` \| `ambiguous` \| `no-version` |
| reason | `reason` | only with `unchecked`: `disabled`, `unreachable`, `unkeyable` |
| self-reported version | `binary-version` | only when the binary reports one that differs from the closure's |

Validation: `confirmed` ⇒ the component's `purl` equals `candidate`. Any other
status ⇒ the component's `purl` is unchanged from what the closure gave it.

## `ContentsOutcome` (new) — what the pass returns

| field | type |
| --- | --- |
| `components` | `Vec<ResolvedComponent>` — recovered modules, deduplicated by PURL |
| `relationships` | `Vec<Relationship>` — member → module, `DependsOn`, source `nix-closure-contents`, data type `dependency` |
| `member_states` | `BTreeMap<(String, String), ReadState>` — keyed by member `(name, version)` |
| `identities` | `Vec<IdentityCandidate>` — to confirm, then apply |
| `record` | `ContentsRecord` |

## `ContentsRecord` — the C213 value

```json
{
  "schema": "waybill-nix-closure-contents/v1",
  "members": 493,
  "read": 2,
  "partial": 0,
  "absent": 480,
  "failed": 0,
  "no-outputs": 11,
  "yielded": 1,
  "modules": { "cargo": 0, "golang": 163 },
  "root": "read"
}
```

- `members` = `read + partial + absent + failed + no-outputs` (asserted).
- `yielded` ≤ `read + partial`.
- `root` ∈ `read`, `partial`, `absent`, `failed`, `no-outputs`, `none`
  (the closure has no root component), `ambiguous`.
- `modules` keys are sorted; an ecosystem with zero is still listed so "none
  found" and "not looked for" differ.

## Relationships between entities

```
closure derivation ──(pname,version)──▶ member component ──DependsOn──▶ recovered module
        │                                      │
        └── outputs[*].path ─▶ store path      ├── C214 read state
                                   │           └── C215 binary identity (when a binary names itself)
                                   └─▶ EmbeddedContents
```

A recovered module reached from several members is one component with one
edge from each. Its `artifact_presence` is `NotInArtifact` only when every
member reaching it is `build-tooling` or `unreferenced`.
