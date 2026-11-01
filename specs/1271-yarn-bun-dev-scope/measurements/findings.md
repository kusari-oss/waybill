# Where the development axis lives, per npm-ecosystem lockfile

Measured 2026-11-01 with `lockfile-dev-axis-probe.sh` in this directory.
Run from a clean directory three times across two rounds; identical every
time. Tool versions:
yarn 1.22.22 (`# yarn lockfile v1`), yarn 4.18.1 (`__metadata: version: 10`),
bun 1.4.2 (`"lockfileVersion": 2`).

Issue #1297 asked for this. Its pnpm v9 claim was labelled inference, and
the same question for yarn and bun was unanswered.

| project shape | reader | dev axis in the lockfile | where |
|---|---|---|---|
| single | yarn v1 | **absent** | substring `dev` occurs 0 times in the file |
| single | yarn Berry | **absent** | the root workspace entry merges both into one `dependencies:` map |
| single | bun | present | `workspaces."".devDependencies` |
| monorepo | yarn v1 | **absent** | the member workspace is not named in the file at all |
| monorepo | yarn Berry | **absent** | member named as `"member@workspace:packages/member"`, deps merged |
| monorepo | bun | present | `workspaces."packages/member".devDependencies` |
| monorepo, object-form `workspaces` | all three | unchanged from the row above | each manager accepts `workspaces: {packages: [...], nohoist: [...]}` |

## What this settles

**bun is the pnpm v9 case again.** The axis is in the lockfile, for the
root and for every workspace member, keyed by workspace path. No manifest
on disk is needed. `workspaces` is already walked for workspace detection
at `bun_lock.rs:214`; only the dev section is unread.

**yarn cannot be served from the lockfile, in either dialect.** This was
the result worth measuring: Berry *looks* like it should carry the axis,
since it records a `"<name>@workspace:<path>"` entry per workspace with an
explicit `dependencies:` map — but that map is the union of the
manifest's `dependencies` and `devDependencies`, so the distinction is
discarded at write time. For v1 and Berry alike the only source is a
`package.json` on disk, which `read_yarn_lock` already reads
(`yarn_lock.rs:44`) for milestone 181's optional classifier.

**A member's dev dependency reaches the lockfile; only its dev-ness is
lost.** In all three managers and both monorepo forms, `tiny-invariant` —
declared only under a workspace member's `devDependencies` — is resolved
into the lockfile as an ordinary entry. So the package is present and
unclassified, not missing. That is what makes workspace-member coverage
worth the glob expansion rather than cosmetic: without it a monorepo emits
the dependency and cannot say it is development-only.

**Both `workspaces` spellings are accepted.** yarn v1, Berry and bun all
honoured the object form `{"packages": ["packages/*"], "nohoist": [...]}`
as well as the bare array, resolving and hoisting the member's dependencies
either way. An implementation that expands those globs must handle both
spellings.

**Berry locates workspace members; v1 does not.** Berry's
`resolution: "member@workspace:packages/member"` gives the member's path
directly, so each member manifest can be found without expanding the root
manifest's `workspaces` glob patterns. A v1 lockfile contains no trace of
the member, so v1 member coverage requires glob expansion or nothing.

## Two incidental findings

bun now writes `"lockfileVersion": 2` and a `"configVersion": 1` field
that no in-repo fixture has; every committed bun fixture is version 1.
`read_bun_lock` (`bun_lock.rs:159`) has no version gate and parses either,
but no test demonstrates that, so the fixture added here is version 2.

#1297's reader table omits `bun_lock.rs` entirely, so the issue
understates the gap by one reader. Verified on `6bbd51e`:
`LifecycleScope::Development` assignment sites are `package_lock.rs` 3,
`pnpm_lock.rs` 5, `yarn_lock.rs` 0, `bun_lock.rs` 0. Posted to #1297
as T039, together with the three measured rows above, which answer
the "Inference, not measured" section that issue left open.

## Not measured

Whether either package manager's *older* lockfile formats carry the axis.
Only the three versions above were generated. A bun lockfileVersion 1 file
and a Berry `__metadata: version` below 10 are untested here; the in-repo
fixtures cover bun version 1 by existing precedent, not by this probe.

## Two observations from implementing US2

Both are about waybill's own reader, found while implementing the yarn
arm, and both changed a test rather than the implementation.

**The `patch:` limitation is stronger than the spec assumed.** FR-012
records that a `patch:`-wrapped registry package goes unclassified. It
is in fact never emitted as a component at all: a `patch:` descriptor
carries a second `@` — `name@patch:name@npm%3A1.0.0#...` — and
`descriptor_to_name` takes the *last* one, so the name it derives is
`name@patch:name` and `build_npm_purl` rejects it. SC-008 holds either
way, and the test now asserts the measured shape rather than the
assumed one.

**FR-003's guard is reachable, but not by the obvious fixture.** The
guard that stops this feature overwriting a milestone 181 `Optional`
assignment survived being deleted, because a name declared under
`optionalDependencies` lands in the *protected* set and the
subtraction removes it long before assignment. The guard matters only
where milestone 181 derived `Optional` from somewhere the root
manifest's three dependency sections do not name: a parent entry's
`optionalDependencies:` sub-block in v1, or `dependenciesMeta` in
Berry. Both arms now have a fixture
(`dev_scope_optional_meta_v1`, `dev_scope_optional_meta_berry`), and
deleting the guard now fails. Without them the guard would have
shipped as untested code that looked tested.

## A third observation: bun seeds without a manifest, but nothing spreads

FR-004 is right that bun needs no `package.json` to *classify*. It is
not the whole story end to end, and the US1 fixture hid the rest.

Scanning `bun_lock/dev_scope_single` with the lockfile alone, the
reader seeded `waybill-fixture-devtool` correctly and
`waybill-fixture-devchild` — a dev-only transitive — stayed unscoped.
Milestone 1269's propagation declines to assign into an ecosystem with
no resolvable root (`scope_propagation.rs`, `ecosystems_with_root`),
and a single-project bun scan has no PURL-shaped main module unless a
manifest exists: the synthesised root came out as
`dev_scope_single@0.0.0`. The monorepo fixture was unaffected because
the bun reader synthesises a workspace-root component from
`workspaces."".name`, which does yield a PURL.

Tested rather than argued. Copying the same lockfile beside a
two-line `package.json`:

```
waybill-fixture-devtool   1.0.0  development
waybill-fixture-devchild  1.0.0  development      <- was unset
waybill-fixture-shared    2.0.0  development      <- was unset
waybill-fixture-shared    1.0.0  (unset)             runtime-reachable
root ref: pkg:npm/waybill-fixture-bun-single@1.0.0
```

So: not a field defect — bun writes its lockfile *from* a manifest, so
a `bun.lock` with no `package.json` beside it is an abnormal shape —
but a fixture that understated the feature by one reader and two
components. The fixture now carries a manifest, and the no-manifest
guarantee is pinned by two tests that copy the lockfile alone into a
temporary directory, where the absence is a property of the test
instead of a property of a directory someone may later add a file to.

## T042 — the nine cells re-measured at the end of implementation

Re-run 2026-11-01 against yarn 1.22.22, yarn 4.18.1 (Berry) and bun
1.4.2 — the same versions as the original probe. Every cell still
reads as this document records it:

- yarn v1, all three project shapes: **dev axis absent**, count of the
  substring `dev` in the lockfile = 0.
- yarn Berry, all three shapes: **dev axis absent**, count = 0;
  `__metadata: version: 10`; and the member entry's `dependencies:`
  map still **merges** both declaration kinds —
  `ms: "npm:2.1.3"` (a dependency) and `tiny-invariant: "npm:1.3.3"`
  (a devDependency) side by side under one key.
- bun, all three shapes: `"lockfileVersion": 2` with
  `"configVersion": 1`, and `workspaces."packages/member"` still
  carries its own `devDependencies` map.

No package manager shipped a new lockfile version mid-implementation,
so the fixtures describe formats that still exist.

## A duplicate Berry parse, measured and removed

Berry member discovery first read the member paths by deserializing the
lockfile with `serde_yaml` a second time — `parse_berry` has already
parsed it — purely to read one `resolution` field per entry. Suspected
while reviewing, then measured rather than argued about, on a synthetic
Berry lockfile of 1,044,954 bytes / 4000 packages
(`scratchpad/bigberry`, regenerable from the script in this feature's
history).

Full `sbom scan` wall time, debug build, sorted runs:

| condition | runs | median |
|---|---|---|
| second `serde_yaml` parse (as first written) | 1.28 1.28 1.28 1.37 4.22 | **1.28s** |
| second parse stubbed out (control) | 0.81 0.83 0.84 0.87 1.00 | **0.84s** |
| line scan of the `resolution` field (shipped) | 0.76 ×6, 1.04 | **0.76s** |

The duplicate parse cost **0.44s of a 1.28s scan — about a third of the
total, and ~52% on top of the work that had to happen anyway.** The
line scan recovers all of it; it lands 0.08s under the stub control,
which is inside the run-to-run spread, so the remaining difference is
noise rather than a further gain.

This is a debug build, so the absolute numbers are inflated and only
the ratio is the finding. Threading the parsed document out of
`parse_berry` would be the better fix structurally; a line scan reads
the same `resolution` field the classification contract names, and the
v1 parser beside it is line-oriented already.

## The optional axis, measured across five real managers (T036 follow-up)

T036's cross-reader table recorded one component classified `optional`
by the package-lock reader and unset by yarn v1, yarn Berry and pnpm v9,
and called it a pre-existing three-reader gap. That reading was built on
hand-written fixtures, and a hand-written fixture missing a marker looks
exactly like a reader that cannot read one. So before filing anything,
the question was put to the package managers themselves:
`optional-axis-probe.sh` in this directory, one manifest declaring a real
dependency under `optionalDependencies` only, five managers, run
2026-11-01 against npm 10.9.8, pnpm 12.10.1, yarn 1.22.22, yarn 4.18.1
and bun 1.4.2.

**Four of the five lockfiles do record the axis — each in a different
place, and only one of those places is where waybill looks.**

| manager | axis in the lockfile | where the manager writes it | where the reader looks | scan result |
|---|---|---|---|---|
| npm | yes | `packages."node_modules/<n>".optional: true` | the same field, `package_lock.rs:106` | **`optional`** |
| pnpm v9 | yes | **`snapshots."<n>@<v>".optional: true`** | the **`packages:`** entry, `pnpm_lock.rs:384` | (unset) |
| yarn v1 | **no** | nothing — zero occurrences of the substring `optional` | n/a | (unset) |
| yarn Berry | yes | the lockfile's root workspace entry, `dependenciesMeta.<n>.optional: true` | **`package.json` >** `dependenciesMeta`, `yarn_lock.rs:183-192` | (unset) |
| bun | yes | `workspaces."".optionalDependencies` | a `packages` entry's own `optionalDependencies` / `optionalPeers` sections, `bun_lock.rs:731` | (unset) |

The scan column is measured, not derived: each probe directory was
reduced to its manifest plus its one lockfile (so `npm/mod.rs`'s
single-reader precedence chain could not route the scan elsewhere) and
scanned with `sbom scan --format cyclonedx-json`, reading the
`waybill:lifecycle-scope` property.

### What this changes

**It is four readers, not three.** bun belongs in the list, exactly as
it belonged in #1297's.

**pnpm v9 is the #1297 defect again, one field over.** `is_optional` is
read from the `packages:` entry, and v9 puts `optional: true` under
`snapshots:`. The same reader, the same dialect, the same shape of
mistake that #1314 fixed for `dev:` — and the comment at
`pnpm_lock.rs:380-383` describes the marker as something "pnpm sets on
entries reachable only through optional edges" without naming the
section, so nothing in the file contradicts the code.

**Berry's axis is in the lockfile, and waybill reads the manifest.**
Milestone 181 reads `package.json > dependenciesMeta`. Berry *derives*
`dependenciesMeta.<n>.optional` into its lockfile from the manifest's
`optionalDependencies` map; an ordinary project's `package.json` has no
`dependenciesMeta` at all. So the m181 Berry path classifies only
projects that hand-write the field — which is precisely what the m181
fixtures do (`yarn_lock.rs:1652`, `:1712`), and why the gap survived a
reader that has tests for it.

**Only yarn v1 is unfixable from the lockfile**, and it is the one case
where the manifest's `optionalDependencies` map is the sole evidence —
the same position milestone 1271 put the development axis in, so the
mechanism to fix it is now in the file.

Out of scope for milestone 1271: FR-003 forbids this feature demoting
an existing optional assignment, and does not ask it to make one. Filed
as **#1324**, which is the issue number SC-003 requires against the one
cross-reader difference this feature leaves standing. The adjacent
`include_dev` asymmetry found in the same readers is **#1323**.
