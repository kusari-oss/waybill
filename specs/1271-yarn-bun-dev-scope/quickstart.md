# Quickstart

## Re-run the format probe

```sh
specs/1271-yarn-bun-dev-scope/measurements/lockfile-dev-axis-probe.sh /tmp/probe
```

Needs `yarn` and `bun` on `$PATH` and network access to the registry. Prints
nine cells — three package managers across three project shapes — and the
whole of each lockfile. Re-run it when either tool releases a new lockfile
version; an undocumented format is one that can move without notice.

## Verify the gap still exists

```sh
for f in package_lock pnpm_lock yarn_lock bun_lock; do
  printf '%-16s %s\n' "$f" \
    "$(grep -c 'LifecycleScope::Development' \
       waybill-cli/src/scan_fs/package_db/npm/$f.rs)"
done
```

On `6bbd51e`: 3, 5, 0, 0. The two zeros are this feature.

## The reference implementation

`npm/pnpm_lock.rs:200`, `v9_importer_scopes` — milestone 1270. Read it before
writing either reader; `contracts/classification.md` is a restatement of what
it already does, and the point of the contract is that all three agree.

## Where the evidence comes from

bun: the lockfile's `workspaces` map, no file on disk needed.
yarn: `package.json`, which `read_yarn_lock` already reads at
`yarn_lock.rs:44` for the milestone 181 optional classifier. Neither yarn
dialect records the axis — confirmed, not assumed, in
`measurements/findings.md`.

## Run the gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

Default features, no `+toolchain` argument (#1189). Assert on the positive
line, never on the absence of FAILED.

## Two things that will waste an afternoon otherwise

**Only one lockfile reader runs per project.** `npm/mod.rs:252-266` is an
`if/else-if` chain: package-lock → pnpm → bun → yarn. A fixture carrying
both `package-lock.json` and `yarn.lock` never reaches the yarn reader, so a
test written over that shape exercises nothing. Cross-reader comparison
(SC-003) needs one fixture directory per lockfile type over one shared
manifest.

**The dev flag already arrives and is thrown away.** `read_yarn_lock` and
`read_bun_lock` both take `_include_dev: bool` and ignore it. The
`ExclusionSet` does not arrive at all — it stops at `npm/mod.rs`, so FR-011
needs it threaded into the yarn reader.
