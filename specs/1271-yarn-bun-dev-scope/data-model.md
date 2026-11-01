# Data model

No new persisted type and no change to any emitted schema. Everything below
lives on the stack for the duration of one reader invocation and is dropped
when it returns.

## Existing types, reused unchanged

**`LifecycleScope`** (`waybill-common`) — the enum this feature finally
assigns in two more readers. The `Development` variant already exists and is
already carried through all three output formats; `Optional` is assigned by
both readers today and must keep its meaning (FR-003).

**`PackageDbEntry`** — the per-package record each reader returns. Its
`lifecycle_scope: Option<LifecycleScope>` field is the one this feature
populates. No field is added.

**`ExclusionSet`** (`scan_fs/package_db/exclude_path.rs`) — consulted via
`matches(candidate_rel_path) -> bool` for FR-011. Not modified; it gains a
caller.

## Per-invocation values

**Workspace declaration set** — one workspace's direct declarations, split
three ways: development, runtime, optional. For bun, one of these per key in
the lockfile's `workspaces` map. For yarn, one per manifest read from disk:
the root, plus each discovered member.

| field | meaning |
|---|---|
| `path` | workspace path; `""` for the root in bun's map, a relative directory for a member |
| `development` | identities declared under `devDependencies` |
| `runtime` | identities declared under `dependencies` |
| `optional` | identities declared under `optionalDependencies` |

**Package identity** — canonical name plus resolved version, under each
reader's existing normalization. Not a new newtype: bun keys its `packages`
map by name and yarn resolves descriptors to a bare name plus version, so
each reader forms the identity its own lockfile can be looked up by. FR-012
requires that an identity be discarded unless the lockfile's package map
contains it, which is also what prevents a non-registry protocol from
synthesising one.

**Scope sets** — the computed result, mirroring milestone 1270's
`V9ImporterScopes`:

| field | meaning |
|---|---|
| `development` | identities declared development by some workspace and reached by no runtime or optional path |
| `protected` | the transitive closure of every runtime and optional declaration |

The invariant is that these are disjoint, and that `development` is the
smaller set after subtraction — never before it. A test asserting
disjointness is cheaper than reasoning about it and is the kind of argued
invariant this project has learned to pin.

**Member location** — for yarn only: a workspace member's directory, with
how it was found, because the two dialects find them differently and a
diagnostic needs to name the source.

| field | meaning |
|---|---|
| `dir` | the member's directory |
| `origin` | lockfile member entry (Berry), or root-manifest pattern expansion (v1) |
| `outcome` | classified, unreadable manifest (FR-010), no manifest present (normal), or excluded by operator (FR-011) |

`outcome` exists so the diagnostics in `contracts/diagnostics.md` are derived
from one enumerated value rather than from scattered conditionals — four
outcomes: two silent (classified, and a pattern that matched a directory with
no manifest) and two that emit a diagnostic (an unreadable manifest under
FR-010, and an operator-excluded path under FR-011).
