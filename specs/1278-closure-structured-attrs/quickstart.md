# Quickstart: 1278-closure-structured-attrs

## Reproduce the defect in five minutes

Needs `nix` on the host. The corpus checkout is already cached if the corpus
harness has run; otherwise clone `MercuryTechnologies/moat` at
`d06905558ba68b49b17f36c875e97f7234a9c19b`.

```sh
REPO=~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo

# The same invocation closure_argv builds. Evaluating the linux attribute from
# macOS is fine -- `derivation show` evaluates, it does not build.
nix derivation show -r --option allow-import-from-derivation false \
    "$REPO#packages.x86_64-linux.default" > /tmp/closure-linux.json

python3 -I specs/1278-closure-structured-attrs/measurements/role-census-probe.py \
    /tmp/closure-linux.json
```

Expect `unreferenced` to fall by 34 derivations between the two arms, and 37
derivations to change role.

## The one check to run first

Before trusting any of it, confirm the probe reproduces the shipped
classifier. The `env`-only arm must agree with the committed golden:

```sh
python3 -I - <<'PY'
import json, collections
g = json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/cdx.json'))
roles = collections.Counter(
    p['value'] for c in g.get('components', [])
    for p in (c.get('properties') or [])
    if p.get('name') == 'waybill:closure-role')
print(dict(roles))        # {'artifact-input': 231, 'both': 34, 'build-tooling': 80, 'unreferenced': 17}
print(sum(roles.values()))# 362
PY
```

347 of 348 shared components agreed when this was written. That agreement is
the evidence the blind spot is live in CI: if CI's nix emitted the flat form
for these derivations, CI's roles would match the **widened** arm instead.

If the agreement is much lower when you run it, stop. Either the goldens have
been refreshed since, or nixpkgs has moved, and every count in the spec needs
re-measuring before it is used.

## See the false native claim

```sh
python3 -c "import json;print(json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/cdx.json'))['compositions'])"
```

The `{"aggregate": "complete", "dependencies": [...]}` record is CycloneDX's
own way of saying "no further dependencies are known to exist", in a document
that skipped 554 declarations. This record is what the disclosure demotes.

## Verify an implementation

1. **Local gate — the whole thing, not a part of it:**

   ```sh
   ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
   ```

   No `+toolchain` argument (#1189). Read `GATE_EXIT` / `GATE_RESULT` from the
   log's last lines, and assert on the positive signal plus the per-target
   `test result: ok. N passed; 0 failed` lines. The absence of the word FAILED
   is not evidence: a build that never compiled contains no failures either.

2. **The corpus goldens are not in that gate.** All of them are `#[ignore]`d
   locally, so a green local run says nothing about them. Refresh through the
   `public-corpus.yml` dispatch and read the diff. **Never** set
   `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` locally.

3. **Check the refresh changed what it should and nothing else.** Expect
   movement only in `nix-closure-moat` and only as SC-003 … SC-008 describe,
   plus the disclosure appearing on closure-scanning targets. A diff in a
   non-closure target means the widening leaked past the type it is confined
   to — see research R6.

4. **Prove the guard can fail.** Remove the typed-form reads and confirm the
   US4 guard goes red. A guard that only passes after the fix is not evidence
   that it can fail, which is what SC-010 is for.

## Traps

- **A typed-form test that only asserts a parsed field passes while the
  feature is broken.** The store-path join is a second, silent failure: output
  paths carry no `/nix/store/` prefix and the referencing fields do. Assert on
  a resolved *role*, not on a resolved field.
- **An empty list is not a reference.** `buildInputs: []` is on all 56
  `source` derivations in this closure. A fix that credits it manufactures
  edges and will look like an improvement in the census.
- **`artifact-input` does not mean "ships".** nixpkgs' Haskell builder passes
  test dependencies through `buildInputs`, which is why `hspec` is an artifact
  input and does not ship. Correcting a role to `artifact-input` must not
  become a `required` scope — the existing rule already forbids it.
- **Don't widen the field list while you are here.** Reading all ~20 nixpkgs
  dependency attributes is #1346. Doing both at once moves the census for two
  reasons and makes SC-004 uncheckable.
- **The counts are dated, not eternal.** They were measured on 2026-11-04 with
  Determinate Nix 3.20.0 and nixpkgs `a799d3e3`. SC-001 and SC-002 are the
  invariants; re-measure the rest rather than quoting them.
