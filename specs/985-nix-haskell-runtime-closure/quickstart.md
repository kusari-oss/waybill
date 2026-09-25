# Quickstart — milestone 985 (issue #962)

How to exercise, verify and debug the transitive runtime closure.

---

## 1. Get a project with the right shape

Needs a `flake.lock` pinning nixpkgs to an exact revision, and Haskell
dependencies. The in-tree corpus target qualifies:

```bash
# already pinned in waybill-cli/tests/corpus_harness_195/manifest.rs
git clone https://github.com/haskell/haskell-language-server /tmp/hls
git -C /tmp/hls checkout --detach 1b4b3c6bdd2bf8d1e1182e2e770f5dea9198db80
```

The decisive property is `flake.lock`'s nixpkgs node having `locked.rev` set —
`original.rev` may be absent, and usually is.

---

## 2. Scan, with and without

```bash
cargo build --release -p waybill --bin waybill

# closure on (the default)
./target/release/waybill sbom scan --path /tmp/hls \
  --format cyclonedx-json --output cyclonedx-json=/tmp/on.cdx.json \
  --no-deps-dev --no-clearly-defined

# closure off — must be byte-identical to pre-feature output
./target/release/waybill sbom scan --path /tmp/hls \
  --format cyclonedx-json --output cyclonedx-json=/tmp/off.cdx.json \
  --no-deps-dev --no-clearly-defined --no-nixpkgs-haskell-closure
```

`--no-deps-dev --no-clearly-defined` keep the run hermetic and fast; without
them ClearlyDefined alone is 97–98% of a cold scan's wall clock (#930).

---

## 3. Check the things that actually go wrong

### Component counts and origin

```bash
jq '[.components[] | select(.purl | startswith("pkg:hackage/"))] | length' /tmp/on.cdx.json
jq -r '.components[] | select(.purl | startswith("pkg:hackage/"))
       | (.properties // [])[] | select(.name=="waybill:nixpkgs-component-origin") | .value' \
   /tmp/on.cdx.json | sort | uniq -c
```

Expect roughly 2.4× the `off` count on this target, split into `declared` and
`transitive`. **Every** Haskell component should carry the origin property — a
component without one is FR-006a failing.

### The closure summary

```bash
jq -r '.metadata.properties[] | select(.name=="waybill:nixpkgs-haskell-closure") | .value' \
   /tmp/on.cdx.json | jq .
```

### Dangling edges — the failure this feature is most likely to cause

```bash
jq -r '
  ([.components[]."bom-ref"] + [.metadata.component."bom-ref"]) as $refs
  | [ .dependencies[] as $d | $d.dependsOn[]? | select(. as $t | $refs | index($t) | not) ]
  | length' /tmp/on.cdx.json
```

**Must be 0.** Anything else is invariant I2 violated, which is milestone 980
recurring — identities rewritten without rewriting edge endpoints. See contract
C-4.3 and the `apply_renames` helper.

### Nothing moved when the closure is off

```bash
# mask the volatile fields, then compare
mask() { jq 'del(.serialNumber) | del(.metadata.timestamp)' "$1"; }
diff <(mask /tmp/off.cdx.json) <(mask /tmp/<pre-feature>.cdx.json) && echo "C-7 holds"
```

Capture the pre-feature file **before** implementing, or from the committed
corpus goldens before they are regenerated. After regeneration this comparison
no longer exists (contract verification obligation 3).

---

## 4. Check against the oracle, not against ourselves

The point of SC-002 is agreement with something that does not share waybill's
code.

```bash
cd specs/985-nix-haskell-runtime-closure/measurements

# declared names from the scan
jq -r '.components[] | select(.purl|startswith("pkg:hackage/")) | .name' /tmp/off.cdx.json \
  | sort -u > /tmp/names.txt

# what nix itself says the runtime closure is
./nix_closure_oracle.sh cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73 ghc910 /tmp/names.txt \
  | jq 'length'
```

Compare with the `transitive` + `declared` total. They agreed exactly (167) on
one project during Phase 0; a disagreement is either a parser bug or an aliased
name (#984) and must be explained, not averaged away.

---

## 5. Run the tests

```bash
# the closure's own suite
cargo +stable test -p waybill --test nix_haskell_resolution_m926

# invariant I2, every ecosystem, ~2s
cargo +stable test -p waybill --test document_integrity

# the full gate — both commands, always
./scripts/pre-pr.sh
```

The corpus target is `#[ignore]`d by default:

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 cargo +stable test -p waybill --test public_corpus \
  corpus_haskell_language_server -- --include-ignored
```

---

## 6. Regenerating the corpus goldens

The existing Haskell target's goldens **will** change substantially. Follow
`docs/development/refreshing-corpus-goldens.md` — in particular:

- generate through CI, never locally (rule zero)
- read every diff and attribute each category before accepting
- confirm reproducibility with a second regeneration *before* committing
- prove the lane can still fail afterwards, and note that the version-constant
  mutation the doc used to recommend is masked and no longer works

---

## Debugging notes

| symptom | first thing to check |
|---|---|
| closure resolves nothing | is `locked.rev` present in `flake.lock`? The gate is the **lock**, not `original` (m926 FR-012) |
| everything resolves but the graph is empty | edges built before identities were rewritten — contract C-4.3 |
| counts differ between two runs | non-determinism in iteration order; every collection in the data model is a `BTree*` for this reason |
| a name resolves offline but not online, or vice versa | the offline check belongs **inside** retrieval, below the cache read (milestone 975) |
| one name disagrees with `nix eval` | almost certainly a configuration alias — see #984 and research R6 |
