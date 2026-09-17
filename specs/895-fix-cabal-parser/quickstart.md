# Quickstart: verifying the `.cabal` parsing fix

## 1. Build the two binaries you will compare

The before-side must be a build of `main` as it stands, kept beside the
after-side. Do not re-resolve `waybill` on `$PATH` — it may be an old install.

```bash
git stash            # or: build from a clean main worktree
cargo build --release -p waybill
cp target/release/waybill target/release/waybill-baseline
git stash pop
```

## 2. Materialise the #891 reproducer

Synthetic names per the fixture policy. Any `src/X.hs` and `Main.hs` will do
— the parser never reads them.

```bash
mkdir -p /tmp/cabal-repro/src && cd /tmp/cabal-repro
echo 'module X where' > src/X.hs && echo 'main = return ()' > Main.hs
cat > repro.cabal <<'EOF'
cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1
build-type:     Simple

library
  hs-source-dirs:
      src
  build-tool-depends:
      waybill-fixture-tool:waybill-fixture-tool
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
    , waybill-fixture-vec >=0.12 && <0.14
  default-language: Haskell2010

executable waybill-fixture-demo
  main-is:             Main.hs
  build-depends:       waybill-fixture-core >=4.14 && <4.15
                     , waybill-fixture-cmt
  -- hs-source-dirs:
  default-language:    Haskell2010
EOF
```

Both layouts are present deliberately: the `library` stanza is hpack-shaped
(field alone on its line, entries below), the `executable` stanza is
`cabal init`-shaped (first entry inline, continuation aligned under it). A
fix that handles only one is not a fix — see research R1.

## 3. Establish the before-side

```bash
target/release/waybill-baseline --offline sbom scan --path /tmp/cabal-repro \
  --format cyclonedx-json --output cyclonedx-json=/tmp/before.json
jq -r '.components[].purl' /tmp/before.json | sort
```

Expect four components, three malformed:

```
pkg:hackage/waybill-fixture-cmt@--_hs-source-dirs:_default-language:____Haskell2010
pkg:hackage/waybill-fixture-core@>=4.11_&&_<4.22
pkg:hackage/waybill-fixture-tool:waybill-fixture-tool@build-depends:_waybill-fixture-core_>=4.11_&&_<4.22
pkg:hackage/waybill-fixture-vec@>=0.12_&&_<0.14_default-language:_Haskell2010
```

**If this does not reproduce, stop.** Either the reproducer is wrong or the
defect has moved; either way the rest of the procedure is measuring the wrong
thing.

## 4. The after-side

```bash
target/release/waybill --offline sbom scan --path /tmp/cabal-repro \
  --format cyclonedx-json --output cyclonedx-json=/tmp/after.json
jq -r '.components[].purl' /tmp/after.json | sort
```

Expect exactly (contract A-1, A-3, A-6):

```
pkg:hackage/waybill-fixture-cmt
pkg:hackage/waybill-fixture-core
pkg:hackage/waybill-fixture-tool
pkg:hackage/waybill-fixture-vec
```

## 5. The measurement that generalises — do not skip it

The four-line diff above is a spot check on one file. Contract A-8 requires
comparing against the **file**, not against an expected list someone typed:

```bash
comm -23 \
  <(jq -r '.components[].purl' /tmp/after.json | sed 's|pkg:hackage/||' | sort -u) \
  <(grep -oE '^[ ,]*[A-Za-z][A-Za-z0-9-]*' repro.cabal | tr -d ' ,' | sort -u)
```

Empty output means no emitted name is absent from the file. Any line is a
fabricated component.

## 6. Constraints must still be reachable

Removing the constraint from the identifier makes C20 the only carrier
(research R3). Verify in all three formats, not just CycloneDX:

```bash
target/release/waybill --offline sbom scan --path /tmp/cabal-repro \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=/tmp/a.cdx.json \
  --output spdx-2.3-json=/tmp/a.spdx.json \
  --output spdx-3-json=/tmp/a.spdx3.json

jq -r '.components[].properties[]? | select(.name=="waybill:requirement-ranges") | .value' /tmp/a.cdx.json
jq -r '.packages[].annotations[]?.comment' /tmp/a.spdx.json | grep requirement-ranges
jq -r '.["@graph"][] | .statement? // empty' /tmp/a.spdx3.json | grep requirement-ranges
```

Each must show `>=4.11 && <4.22` and `>=0.12 && <0.14` verbatim.

## 7. Teeth-check every new test

Per SC-008, a test that passes before the fix proves nothing. Revert only the
production hunks — keeping the tests — and confirm each new test fails, and
fails *for its own reason* rather than by failing to compile:

```bash
cp waybill-cli/src/scan_fs/package_db/haskell.rs /tmp/haskell.rs.fixed
git checkout main -- waybill-cli/src/scan_fs/package_db/haskell.rs
cargo test --test <the new integration target> 2>&1 | grep -E "^test |panicked"
cp /tmp/haskell.rs.fixed waybill-cli/src/scan_fs/package_db/haskell.rs
```

This project has shipped a test that passed against the defect it was written
to catch. Do not assume; run it.

## 8. Confirm nothing else moved

```bash
./scripts/pre-pr.sh
git status --short     # expect no changes under tests/fixtures/
```

No committed golden should change — no Haskell project is in either corpus
today (research R5). If one does change, stop and find out why before
regenerating it.

## 9. The corpus target (second phase only)

Goldens are generated **in CI**, never locally — they embed runner-absolute
paths, and a locally-generated golden passes only on the machine that made
it. Follow `docs/development/refreshing-corpus-goldens.md`, and dispatch one
run at a time: the lane's concurrency group is shared, so a second pending
dispatch cancels the first.
