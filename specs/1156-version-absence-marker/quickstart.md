# Quickstart — feature 1156

## Run the matrix hermetically (no network, default lane)

```sh
cargo test -p waybill --test public_corpus
```

The invariant matrix reads committed goldens from
`waybill-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`,
so it runs in the ordinary test lane. This is the loop for everything
except the golden refresh.

## See the four cells this feature moves

Before the acceptance change, these four fail I8 with
`versionless-and-unexplained` and are declared:

```sh
grep -n -A3 'invariant = "I8"' \
  waybill-cli/tests/corpus_harness_195/invariant-declarations.toml
```

Expect five I8 blocks before, one (`image-postgres16`) after.

## Inspect an anchor on the wire

```sh
jq '.components[]
    | select(.properties[]? | select(.name == "waybill:component-kind"
                                     and .value == "lockfile-resolve"))
    | {name, version, properties}' \
  waybill-cli/tests/fixtures/public_corpus/pants-example-django/cdx.json
```

After the emission change each such component carries
`waybill:version-absence = by-construction` and an empty `version`.

A trap worth avoiding: a plain `grep lockfile-resolve` over the fixtures
tree also matches `npm-express`, because C151's value string on that target
contains the words "no lockfile-resolved version". Filter on the property
name and value as above, or you will size the change wrong and go looking
for a defect that is not there.

## Refresh the goldens — never locally

Rule zero of `docs/development/refreshing-corpus-goldens.md`. Receipts:
#818 (an arm64 macOS baseline against Linux CI — nine days of failures
across five release tags) and #832 (fixture content depending on whether
the host had git-lfs — three nights, two wrong diagnoses).

```sh
# 1. regenerate on this branch
gh workflow run "Public corpus regression" \
  -f branch=1156-version-absence-marker -f regen_goldens=true

# 2. download the artifact, commit the 12 changed files, then verify
gh workflow run "Public corpus regression" \
  -f branch=1156-version-absence-marker -f regen_goldens=false
```

`-f branch=`, **never** `--ref`. `--ref` silently runs against `main` and
reports success (#1223).

Measured cost: the last three runs took 5m04s, 5m02s and 4m36s warm-cache;
the workflow estimates ~30 min cold and times out at 45. Budget two waits
of roughly five minutes plus the review between them.

## Review the diff before committing it

```sh
cargo run -p xtask -- corpus-diff
```

The review tool **sorts arrays**; the lane compares byte-for-byte after
masking and does **not** tolerate reordering. So a diff this tool calls
clean can still fail the lane. If that happens the fix belongs in
`mask_nondeterministic`, not in the review tool.

Expect exactly 12 files changed, and within them only the seven anchor
components, each gaining one property/annotation. Anything else is a
finding to explain before merge (FR-015).

## Prove the gates have teeth

Four mutations, each of which must fail:

```sh
# 1. the marker removed from one anchor -> I8 red on that target
# 2. the marker's value replaced with garbage -> I8 red (value is pinned)
# 3. a registered must-pass cell forced to NotApplicable -> lane red
# 4. a declaration left behind after the fix -> StaleDeclaration, lane red
cargo test -p waybill --test public_corpus mutation
```

Mutation 3 is the one that matters most: it is the only evidence the new
registry does anything. A registry that cannot fail is the vacuity trap one
level up.

## Pre-PR gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

Check the gate's own exit status, not a pipeline's. Both changes must pass
this independently (FR-018).
