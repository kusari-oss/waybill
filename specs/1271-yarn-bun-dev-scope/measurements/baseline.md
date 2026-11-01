# Baselines

## T010 — the zero baseline (SC-001)

`LifecycleScope::Development` assignment sites per npm lockfile reader.
Read from git rather than from memory, so the pre-change column stays
checkable after the working tree moved:

```sh
for f in package_lock pnpm_lock yarn_lock bun_lock; do
  git show HEAD:waybill-cli/src/scan_fs/package_db/npm/$f.rs \
    | grep -c 'LifecycleScope::Development'
done
```

| reader | `6bbd51e` (pre-change) | working tree |
|---|---|---|
| `package_lock.rs` | 3 | 3 |
| `pnpm_lock.rs` | 5 | 5 |
| `yarn_lock.rs` | **0** | 0 — US2 not yet implemented |
| `bun_lock.rs` | **0** | **3** — US1 |

SC-001's bun half is met; its yarn half is open until US2 lands.

## Honest note on ordering

Phase 2 was specified as blocking and was supposed to precede any source
edit (research R5). It did not: US1's reader change was written first, and
this file was produced afterwards. Recorded rather than quietly fixed,
because the whole point of R5 is that attribution cannot be reconstructed
from a theory about what the state used to be.

What saves the attribution property here is narrow and worth stating:
the change is **uncommitted** and `HEAD` is clean at `6bbd51e`, so the
pre-change state is not a recollection — it is readable, and the table
above was produced by reading it. Had the change been committed first,
the FR-007 distinction between an introduced and a pre-existing
disagreement would already have been lost for bun.

## T011 — cross-reader baseline

Captured 2026-11-01, before any edit to `yarn_lock.rs`. Four fixture
directories over one byte-identical `package.json`, scanned **separately**
because `npm/mod.rs:252-266` runs exactly one lockfile reader per project
root:

```sh
for f in dev_scope_single dev_scope_berry \
         dev_scope_reference_npm dev_scope_reference_pnpm; do
  target/debug/waybill sbom scan \
    --path waybill-cli/tests/fixtures/yarn_lock/$f \
    --format cyclonedx-json --output /tmp/baseline/$f.json
done
```

### Read the right field

The comparison is on the **`waybill:lifecycle-scope` property**, not on
the CDX `scope` field. CDX's three-value enum cannot express the
development-versus-optional split, so `builder.rs:1375-1388` emits
`scope: "excluded"` for *any* non-runtime scope and puts the finer value
in the property. A first pass at this table read `scope` and therefore
reported five disagreements, one of which was an artefact of conflating
`development` with `optional`. Read the property.

### The baseline

| component | yarn v1 | yarn Berry | package-lock | pnpm v9 |
|---|---|---|---|---|
| `waybill-fixture-both` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-devchild` | **(unset)** | **(unset)** | development | development |
| `waybill-fixture-devtool` | **(unset)** | **(unset)** | development | development |
| `waybill-fixture-optional` | (unset) | (unset) | optional | **(unset)** |
| `waybill-fixture-real` | **(unset)** | **(unset)** | development | development |
| `waybill-fixture-runtime` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-shared` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-dev-scope` (root) | absent | (unset) | absent | absent |

### What it establishes

**The #1297 gap, measured rather than asserted.** Over one manifest,
`package-lock.json` and `pnpm-lock.yaml` both classify three components as
`development`; both yarn dialects classify nothing at all. The two working
readers agree with each other exactly, so US2 has an unambiguous target
state rather than a judgement call.

**Development scope propagates past direct declarations.**
`waybill-fixture-devchild` is a transitive of `waybill-fixture-devtool`,
never declared in the manifest, and both working readers mark it
`development`. That is milestone 1269's pass operating on reader seeds,
and it confirms the US1 design decision: a reader that seeds only direct
declarations still reaches the same end state, so duplicating the closure
inside the reader would have been redundant.

### Two pre-existing disagreements, neither caused by this feature

Recorded here per FR-007 and the Q1 clarification: a disagreement this
feature introduces blocks it; one that predates it is measured, recorded
and filed.

1. **`waybill-fixture-optional` — package-lock says `optional`, pnpm says
   nothing.** A pnpm-reader gap of the same family as #1297, in a reader
   milestone 1270 was supposed to have finished. **Needs its own issue; out
   of scope for milestone 1271**, which must not silently "fix" pnpm while
   claiming to fix yarn. Filed as #1324.

   **Correction, made at the end of implementation.** This row first read
   "the declaration is present in pnpm's `importers..optionalDependencies`
   section, so pnpm has the evidence and does not act on it". That
   diagnosis was wrong on both halves. The pnpm reader never consults
   `importers` for the optional axis — it reads `optional: true` from the
   `packages:` entry (`pnpm_lock.rs:384`) — and real pnpm v9 writes that
   marker under `snapshots:`, which this hand-written fixture does not
   carry at all. So the unset value here is partly a fixture artifact and
   partly a reader defect, and the two were indistinguishable until the
   managers were asked directly. See the optional-axis probe in
   `findings.md`.

2. **The root component appears only in the Berry scan.** Berry's lockfile
   names the root workspace (`"…@workspace:."`); the v1, npm and pnpm
   lockfiles do not, so the root resolves to a different identity. This is
   an identity artefact rather than a scope defect, but SC-003's comparison
   must normalise or exclude the root or it will read as a permanent
   mismatch.

### Consequence for SC-003

The criterion is satisfiable: after US2, yarn v1 and Berry must both show
`development` on exactly `devtool`, `devchild` and `real`. The optional
row is expected to keep disagreeing until the pnpm issue above is fixed,
and that is now an enumerated difference with a stated cause rather than
an unexplained one — which is what SC-003 requires.

## T012 — where this feature changes nothing

`npm/mod.rs:252-266` is an `if/else-if` chain in precedence order
package-lock → pnpm → bun → yarn, so exactly one lockfile reader runs per
project root. Consequences for anyone writing a test here:

- A fixture directory holding `package-lock.json` **and** `yarn.lock`
  never reaches the yarn reader. A test over that shape passes while
  exercising none of the code under test.
- The same is true of bun: a directory with `pnpm-lock.yaml` beside
  `bun.lock` reaches pnpm, not bun.
- Cross-reader comparison therefore cannot be a single scan. It is one
  fixture directory per lockfile type over one shared manifest, scanned
  separately, compared afterwards.

---

## T036 — SC-003 verified, post-implementation

Same four fixture directories, same separate scans, same
`waybill:lifecycle-scope` property as the T011 baseline. Captured
2026-11-01 after US1–US3.

| component | yarn v1 | yarn Berry | package-lock | pnpm v9 |
|---|---|---|---|---|
| `waybill-fixture-devchild` | **development** | **development** | development | development |
| `waybill-fixture-devtool` | **development** | **development** | development | development |
| `waybill-fixture-real` (aliased) | **development** | **development** | development | development |
| `waybill-fixture-optional` | (unset) | (unset) | optional | (unset) |
| `waybill-fixture-both` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-runtime` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-shared` | (unset) | (unset) | (unset) | (unset) |
| `waybill-fixture-dev-scope` (root) | absent | (unset) | absent | absent |

**yarn v1 versus yarn Berry: zero differences.** Not one component
differs, which is the half of SC-003 this feature is answerable for.
The three components that were `(unset)` under both yarn dialects in
the T011 baseline — `devchild`, `devtool`, `real` — now match what the
two working readers already said. The #1297 gap is closed for yarn.

Two differences remain, both enumerated in T011 before any source
edit, so neither was introduced here:

1. **A root-manifest `optionalDependencies` declaration is seen only
   by the package-lock reader.** `waybill-fixture-optional` is
   `optional` there and unset in the other three. Broader than the pnpm
   gap T011 recorded, and broader again than this table can show: the
   probe recorded in `findings.md` put the same manifest to five real
   managers and found **four** readers missing it, bun included, each
   for a different reason, with yarn v1 the only one whose lockfile
   does not record the axis at all. Out of scope here — FR-003 forbids
   demoting an optional assignment, it does not ask this feature to
   make one. **Filed as #1324**, which satisfies SC-003's requirement
   that every remaining difference carry a cause and an issue number.
2. **The root identity.** Berry emits its `"<root>@workspace:."` entry
   as a component, so the root appears there and nowhere else. An
   artifact of what each lockfile records about itself; SC-003 is
   evaluated with the root normalised out.

## T037 — SC-004 verified across all four monorepo readers

Member-only development declaration, classified identically by every
reader that can see one:

| reader | development set |
|---|---|
| bun | `devonly`, `devonly-child` |
| yarn v1, `workspaces` as array | `devonly`, `devonly-child` |
| yarn v1, `workspaces` as object | `devonly`, `devonly-child` |
| yarn Berry | `devonly`, `devonly-child` |

The direct declaration is the reader's seed; the child is milestone
1269's propagation. Both `workspaces` spellings reach the member, and
Berry reaches it from the lockfile's own member entry without
expanding a pattern at all.

## T038 — SC-002 component counts, before and after `--exclude-scope dev`

Captured 2026-11-01 on the implementation. The spec quotes no numbers
deliberately; these are them.

| fixture | before | after `--exclude-scope dev` | dropped |
|---|---|---|---|
| `bun_lock/dev_scope_single` | 8 | 5 | 3 |
| `bun_lock/dev_scope_monorepo` | 7 | 5 | 2 |
| `yarn_lock/dev_scope_single` | 7 | 4 | 3 |
| `yarn_lock/dev_scope_berry` | 8 | 5 | 3 |
| `yarn_lock/dev_scope_monorepo_array` | 7 | 5 | 2 |
| `yarn_lock/dev_scope_monorepo_object` | 7 | 5 | 2 |
| `yarn_lock/dev_scope_monorepo_berry` | 10 | 8 | 2 |
| `yarn_lock/dev_scope_symlink_member` | 7 | 5 | 2 |
| `yarn_lock/dev_scope_protocols` | 10 | 10 | 0 |
| `yarn_lock/dev_scope_optional_meta_v1` | 3 | 2 | 1 |
| `yarn_lock/dev_scope_optional_meta_berry` | 4 | 3 | 1 |
| `yarn_lock/dev_scope_no_manifest` | 2 | 2 | 0 |
| `yarn_lock/dev_scope_bad_member` | 7 | 6 | 1 |
| `yarn_lock/dev_scope_reference_npm` | 7 | 4 | 3 |
| `yarn_lock/dev_scope_reference_pnpm` | 7 | 4 | 3 |

Every one of these was 0 before this feature for the two readers it
touches. Three rows are deliberately 0 and stay 0:
`dev_scope_protocols` (every development declaration uses a
non-registry protocol, FR-012), `dev_scope_no_manifest` (no evidence
exists, FR-006), and — read with the two above it —
`dev_scope_bad_member` drops 1 rather than 2 because only one of its
two members has a readable manifest (FR-010).

`yarn_lock/dev_scope_single` drops the same three components as both
reference lockfiles over the same `package.json`, which is SC-003
restated as the thing an operator actually does with it.
