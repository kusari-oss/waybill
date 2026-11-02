# Where each npm-ecosystem manager records the optional axis

Re-measured independently of #1324's table, on this host, with the five
managers installed locally. Every row below was produced by running the
manager, not by reading its documentation.

**Tool versions**: npm 10.9.8, pnpm 12.16.1, yarn 1.22.22, yarn 4.6.0
(via corepack), bun 1.4.2. Node v22.23.1, darwin arm64.

Probe script: `optional-axis-probe.sh` in this directory. Re-run it when
a manager changes format; an undocumented layout is one that can move
without notice.

## Probe A — a root manifest declaring one dependency optional

`dependencies: {ms}`, `optionalDependencies: {lodash.noop}`,
`devDependencies: {escape-string-regexp}`.

| manager | where the root optional declaration appears | reader's current source | classified? |
|---|---|---|---|
| npm | `packages."node_modules/lodash.noop".optional: true` | the same field, `package_lock.rs:106` | **yes** |
| pnpm v9 | `importers.".".optionalDependencies` **and** `snapshots."lodash.noop@3.0.1".optional: true` | `packages:` entry's `optional` key, `pnpm_lock.rs:384` | no |
| yarn v1 | **nowhere** — v1 has no root/workspace entry | n/a | no |
| yarn Berry | lockfile root workspace entry `dependenciesMeta.lodash.noop.optional: true` | `package.json > dependenciesMeta`, `yarn_lock.rs:183` | no |
| bun | `workspaces."".optionalDependencies` | a `packages` entry's own metadata sections, `bun_lock.rs:731` | no |

Confirms #1324's table on all five rows, including the Berry row.

**This contradicts a code comment.** `yarn_lock.rs:127-129` states
"Berry's lockfile does NOT carry the flag (it lives in package.json
only)." Measured, Berry writes `dependenciesMeta` into the lockfile's
workspace entry, derived from the manifest's own map. The comment is
wrong and the reader's source choice follows from it.

The `dev_scope_optional_meta_berry` fixture is self-contradictory on the
same point: its header says the status "lives in package.json's
`dependenciesMeta` and nowhere else", while the fixture body writes
`dependenciesMeta` into the lockfile. Hand-written, so it was never
evidence either way.

## Probe B — does optional propagate through a RUNTIME parent?

`dependencies: {chokidar@3.6.0}` only. `chokidar` is a runtime
dependency that declares `fsevents` in its own `optionalDependencies`.
This is the question of whether the axis is a property of the package or
of the edge.

| manager | `chokidar` (the runtime parent) | `fsevents` (the optional child) | shape |
|---|---|---|---|
| npm | `{}` — not optional | `{"optional": true}` | **precomputed, closed** |
| pnpm v9 | no marker | `snapshots."fsevents@2.3.3".optional: true` | **precomputed, closed** |
| yarn v1 | `optionalDependencies: fsevents "~2.3.2"` on the parent | no marker of its own | edge only |
| yarn Berry | `dependenciesMeta.fsevents.optional: true` on the parent | no marker of its own | edge only |
| bun | `optionalDependencies: {fsevents}` in the parent's metadata | `{"os":"darwin"}` — no optional marker | edge only |

Two findings, both load-bearing for the design:

**1. The semantics are not a free choice.** npm and pnpm independently
mark the child and not the parent, so optional propagates along optional
edges and attaches to the reachable package. Both are reference
implementations of the same ecosystem and they agree. A design that
marked `chokidar` optional, or that left `fsevents` unmarked, would
disagree with both.

Neither probe exercises cancellation — a package reachable by BOTH an
optional and a non-optional path. npm's and pnpm's flags are documented
as "reachable only through optional edges", which implies any
non-optional path cancels, but that is **inference from the field's
description, not something these probes measured.** A probe for it
belongs in the plan phase.

**2. Two arms precompute the closure and three do not.** npm and pnpm
hand over a closed per-package answer; yarn v1, Berry and bun record the
edge on the parent and leave the closure to the reader. So "read a flag"
is the whole fix for pnpm, and the other three need a traversal.

## Probe C — yarn v1, refining #1324

#1324 records yarn v1 as "nothing; zero occurrences of the substring
`optional`". Probe A reproduces that. Probe B does not: with a
transitive optional dependency present, v1 writes

```
chokidar@3.6.0:
  dependencies:
    anymatch "~3.1.2"
    ...
  optionalDependencies:
    fsevents "~2.3.2"
```

So the absence in Probe A is a property of the **probe shape**, not of
the format. A v1 lockfile has no root or workspace entry, so a *root*
declaration has nowhere to go — but v1 does record *transitive* optional
edges, in the same per-entry sub-block shape Berry and bun use.

The consequence for scope: yarn v1 is the only arm whose root
declaration is unrecoverable from the lockfile alone. Its root answer
must come from `package.json`, which the yarn reader already loads for
m159 aliases and m181 optional names.

## Summary — four readers, four different fixes

| reader | root declaration | transitive | closure needed |
|---|---|---|---|
| pnpm v9 | `snapshots.<id>.optional` | same field, free | **no** |
| bun | `workspaces.<p>.optionalDependencies` | parent metadata (already read by m667) | yes |
| yarn Berry | lockfile workspace entry `dependenciesMeta` | parent entry `dependenciesMeta` | yes |
| yarn v1 | `package.json > optionalDependencies` only | parent `optionalDependencies:` sub-block | yes |

pnpm's root and transitive answers are the same field, so they cannot be
delivered separately. The other three can.

## Probe C — does a non-optional path cancel optionality?

Harness: `cancellation-probe.sh`, same five managers and versions.
Added during the plan phase (2026-11-02) to measure the one rule the
first two probes left as inference.

Neither Probe A nor Probe B built the graph shape that decides the
question: a package reachable by **both** an optional and a
non-optional path. Two cases were built, one contested and one control.

- **C1 (contested)** — root declares `debug@4.3.4` under
  `dependencies` and `ms@2.1.2` under `optionalDependencies`. `debug`
  depends on `ms` non-optionally, so `ms` is reachable both ways.
- **C2 (control)** — root declares `debug@4.3.4` under
  `optionalDependencies` and nothing runtime, so `ms` is reachable only
  along an optional edge.

Only npm and pnpm can answer: they are the two that write a precomputed
closure. The other three record the edge on the declaring parent, so
the closure is waybill's to compute and the file has no answer to read.

| case | npm `node_modules/ms` | pnpm `snapshots."ms@2.1.2"` |
|---|---|---|
| C1 contested | `optional: false` | `{}` — no `optional` key |
| C2 optional-only | `optional: true` | `optional: true` |

**Cancellation is confirmed.** In C1 both managers decline to mark `ms`
optional even though the root declared it so, because a non-optional
path reaches it. FR-006 stands as written.

**Transitive propagation is confirmed in the same measurement.** In C2
`ms` is marked optional by both managers although it is declared
optional nowhere — it inherits the classification purely by being
reachable only through an optional edge. FR-007a stands as written.

C1 also printed exactly **one** `ms` node, at 2.1.2. Had `debug`'s range
and the pinned optional range resolved to different versions there would
have been two nodes, the contested shape would not exist, and the case
would be void. It is not.

**The control is load-bearing.** C1 and C2 return *different* answers
from the same harness on the same five managers. Uniform results across
cases that should differ would mean the probe was reporting itself
rather than the managers — the failure mode recorded in
`feedback_a_probe_can_lie_about_the_system`. They differ.

Two further confirmations fall out, both consistent with Probe B:

- Berry's lockfile `dependenciesMeta` sits on the **root workspace
  entry** and names only the root's own declaration — `ms` in C1,
  `debug` in C2. It records the edge, never the closure.
- bun likewise records `optionalDependencies` on the declaring
  workspace and the dependency edge in the parent's metadata, with no
  closure anywhere.

yarn v1 wrote zero occurrences of `optional` in both cases. That is not
a contradiction of Probe C's row in the summary table: `debug` declares
no optional dependencies of its own, so there is no transitive edge for
v1 to record. Probe B's `chokidar`/`fsevents` case remains the evidence
that v1 does record them.

## Probe D — pnpm v9 peer contexts, and the `peer:` key that does not exist

Harness: `peer-context-probe.sh`. Added during **implementation**
(task T063), not Phase 0 — recorded here because the gap is the useful
part of the finding.

Probes A–C asked where the optional marker lives and what it means.
None asked how the peer-precedence guard FR-009 requires reads its input
on v9. Writing the pnpm fix surfaced it, and the answer changed the
implementation twice.

### Finding D1 — a real v9 lockfile has no `peer:` key at all

`pnpm_lock.rs` read `is_peer` from `tbl.get("peer")`, the `packages:`
entry. Across all four probe shapes, **zero** occurrences of a bare
`peer:` key in any `pnpm-lock.yaml`. The guard could never fire.

That was invisible while the optional branch was also dead — both inputs
read as `false`, so the classifier fell through to Runtime for the right
answer by the wrong route. It stops being invisible the moment the
optional branch works.

v9 expresses peer-optionality on the **declaring parent's** `packages:`
entry:

```yaml
packages:
  debug@4.3.4:
    peerDependencies:
      supports-color: '*'
    peerDependenciesMeta:
      supports-color:
        optional: true
```

### Finding D2 — the flag must AND-fold across peer contexts

One canonical identity can appear under several `snapshots:` keys
differing only by a peer-context suffix:

```yaml
snapshots:
  debug@4.3.4(supports-color@9.4.0):
    optional: true
```

`build_snapshots_lookup` strips that suffix via `parse_pnpm_key`, so
several keys collapse to one emitted component. FR-006 says any
non-optional path cancels, so the identity is optional only when
**every** context says so. A last-write-wins insert would make the
answer depend on YAML key order — hence the AND-fold, and hence
`m1272_v9_optional_and_folds_across_peer_contexts` asserts both
orderings.

### Finding D3 — pnpm already applies the cancellation to peers

| shape | `supports-color@9.4.0` |
|---|---|
| p1 — runtime parent, peer also a runtime root dep | `{}` — not optional |
| p2 — optional parent, peer also a runtime root dep | `{}` — not optional |
| p3 — optional parent, optional peer, no runtime path | `optional: true` |
| p4 — runtime parent, peer declared optional at root | `optional: true` |

p1 and p2 are the load-bearing rows: a peer with a runtime path is not
marked, even when its parent is optional. pnpm has already done FR-006's
work. So reading `snapshots.<id>.optional` is correct for peers too, and
no reader-side peer closure is needed.

### What this changed in the implementation

1. `build_snapshots_lookup` AND-folds rather than inserting (D2).
2. `v9_peer_optional_names` reads `peerDependenciesMeta` from the
   `packages:` entries, giving FR-009's guard a real source on v9 (D1).
3. The m180 test `pnpm_peer_optional_stays_peer_not_optional` was
   relabelled from `lockfileVersion: '9.0'` to `'6.0'`, which is the
   dialect its `peer:`/`optional:`-on-a-packages-entry shape belongs to.
   Left as v9 it would still pass after this milestone — but only
   because both flags now read as absent, i.e. for the wrong reason.
   `m1272_v9_peer_optional_stays_peer_not_optional` is the real-shape
   v9 equivalent, and carries a positive control so it cannot pass by
   classifying nothing.

### Two further fixtures found asserting shapes pnpm cannot emit

Both are FR-013's defect class, found by this milestone rather than
designed for:

- `pnpm_optional_true_populates_lifecycle_scope_optional` declared
  `lockfileVersion: '9.0'`, used v6/v7 slash-prefixed keys, put
  `optional: true` in `packages:`, and had no `snapshots:` section.
- `waybill-cli/tests/fixtures/optional_dep/pnpm/pnpm-lock.yaml` does the
  same and repeats the marker in both sections. Its own `package.json`
  description says "hand-authored".

Both now sit beside a measured-shape counterpart
(`tests/fixtures/optional_dep/pnpm_v9_measured/`), per FR-013's rule
that a hand-written shape must not be an arm's only evidence.
