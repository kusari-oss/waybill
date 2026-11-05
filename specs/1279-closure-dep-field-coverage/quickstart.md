# Quickstart: 1279-closure-dep-field-coverage

## Reproduce the baseline

Needs a closure JSON. **Capture it once and read it from a file** — do not
point a probe at a fixture flake and let it evaluate: on 2026-11-04 running a
real `nix` against `waybill-cli/tests/fixtures/nix_eval/ifd` rewrote that
fixture's committed `flake.lock`, which broke 11 tests in an unrelated file
and was then swept into a commit by `git add -A`.

```sh
REPO=~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo

# The same invocation closure_argv builds. Capture BOTH systems: the goldens
# pin linux, and the two disagree by five times on one population.
nix derivation show -r --option allow-import-from-derivation false \
    "$REPO#packages.x86_64-linux.default"  > closure-linux.json
nix derivation show -r --option allow-import-from-derivation false \
    "$REPO#packages.aarch64-darwin.default" > closure-darwin.json

python3 -I specs/1279-closure-dep-field-coverage/measurements/attribute-census.py closure-linux.json
python3 -I specs/1279-closure-dep-field-coverage/measurements/role-delta-probe.py closure-linux.json
```

Expect, on **linux**: 6 populated unconsulted attributes carrying 230
store-path declarations; and a role delta of 7 derivations, census
`270 / 118 / 42 / 1,319 → 268 / 121 / 45 / 1,315`.

**`artifact-input` goes DOWN by two.** Three of its members gain a tooling
reference and become `both` while only one arrives. If you read that as an
error you will go looking for a bug that is not there.

## The control to run first

Before trusting any of it, confirm the probe reproduces the shipped
classifier. Its `before` arm must equal the committed golden's own census:

```sh
python3 - <<'EOF'
import json
d = json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/spdx-3.json'))
for e in d['@graph']:
    st = e.get('statement', '')
    if '"waybill:nix-closure"' in st:
        print(json.loads(json.loads(st)['value'])['roles']); break
EOF
```

That prints the shipped derivation-level census. The probe's `before` arm must
match it **exactly** — not approximately. #1348 built a 347-of-348
per-component comparison when this exact figure was sitting in the artifact;
if the two disagree, re-measure before concluding anything.

## See the defect

```sh
python3 - <<'EOF'
import json
d = json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/cdx.json'))
c = next(x for x in d['components'] if x['name'] == 'numactl')
print({p['name']: p['value'] for p in c['properties'] if 'name' in p}.get('waybill:closure-role'),
      '| scope =', c.get('scope'))
EOF
```

`unreferenced | scope = excluded` — a component nix references into the
artifact through an attribute waybill does not read, claimed absent from it.
After this milestone it is `artifact-input` with no scope.

## Verify an implementation

1. **Local gate**, the whole thing:

   ```sh
   ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
   ```

   No `+toolchain` (#1189). Assert on `GATE_RESULT`, the expect line, and the
   per-target `test result: ok. N passed; 0 failed` lines. The absence of
   FAILED is not evidence.

2. **The corpus is not in that gate, and running it locally is worse than
   not.** The target prints `skipping nix-closure-moat:
   WAYBILL_RUN_PUBLIC_CORPUS not set` and then reports `ok` **in 0.00s** — a
   pass that checked nothing. Refresh with `regen_goldens=true`, install, then
   dispatch again with `regen_goldens=false` and confirm the regen count in
   the log is **0**, so you know it compared rather than rewrote.

3. **Check nothing moved outside the closure target.** Only
   `nix-closure-moat` may change. A non-closure target that moved means the
   widening leaked past the two constants.

4. **Prove the guard can fail.** Remove one newly-consulted attribute from
   the constant and confirm a test goes red. A widened field set no test is
   sensitive to is not evidence (SC-007).

## Traps

- **Join golden `scope` to roles from the SAME system.** The goldens pin
  linux. Computing roles from a darwin closure and reading `scope` from the
  linux golden, joined by component name, is how #1348 reported nine false
  exclusions where there were six — and name keying also collapses same-named
  components of different versions.
- **One variable per comparison.** Measuring a pre/post difference with two
  different fixture directories attributed a nix-rewritten `flake.lock` to a
  code change on 2026-11-04. `diff -r` the two arms before naming a cause.
- **`artifact-input` does not mean "ships".** nixpkgs' Haskell builder passes
  test dependencies through `buildInputs`, which is why `hspec` is an artifact
  input and does not ship. Correcting a role to `artifact-input` must not
  become a `required` scope; m1277's rule already forbids it.
- **An empty attribute is not a reference.** Four are declared on every
  derivation of the darwin closure and hold zero paths.
- **Don't classify stdenv's injected attributes — all three of them.**
  `defaultNativeBuildInputs`, `defaultBuildInputs` and
  `extraNativeBuildInputs` carry most of the unread declarations, and FR-012
  leaves them alone so the census moves only for the four consulted
  attributes and SC-011 stays checkable. Classifying them is a different
  milestone.
  **And enumerate them from both systems, not one.** FR-012's first draft
  named two, taken from the darwin census, where `extraNativeBuildInputs` is
  unpopulated. On linux it carries 3 declarations and `defaultBuildInputs`
  carries none — so the pair was incomplete on the exact closure the criteria
  are stated against. Run the census on both captures before enumerating
  anything.
- **The counts are dated and system-labelled.** SC-001, SC-002 and SC-009 are
  the invariants; everything numbered is linux as of 2026-11-05.
