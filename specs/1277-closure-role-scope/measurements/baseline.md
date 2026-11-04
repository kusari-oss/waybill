# Baseline — what the goldens say before milestone 1277

Taken on `main` at `5a0fa58` (both milestone 1275 and 1276 landed), reading
the **committed corpus goldens** rather than running a scan. Those goldens
were generated in CI by `public-corpus.yml` run 41720889486 and installed
unchanged, so they are the emitted output of the shipped code on the pinned
targets.

#1342's figures were measured on a different fixture —
`waybill-fixture-ifd` at nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`,
1,290 derivations, giving 185 / 37 / 79 / 22. Those are not wrong; they are
a different closure. Everything below is the corpus, because the corpus is
what a success criterion can be asserted against.

## Harness

```sh
python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py
```

## Which targets carry closure roles

```
### nix-closure-moat: 387 components, 362 with a closure-role
    artifact-input     231   pkg:hackage of those: 131
    both                34   pkg:hackage of those: 2
    build-tooling       80   pkg:hackage of those: 1
    unreferenced        17   pkg:hackage of those: 0
    role+scope pairs present today: {('artifact-input', 'excluded'): 9, ('both', 'excluded'): 1}

### haskell-security-advisories: 251 components, 0 with a closure-role
    role+scope pairs present today: NONE

### haskell-language-server: 399 components, 0 with a closure-role
    role+scope pairs present today: NONE
```

**One corpus target exercises this feature.** The other two Haskell targets
do not run `--nix-closure`. This is the single most consequential fact for
the spec: it moves the breadth of the test plan onto synthetic fixtures and
makes the remaining 16 targets a byte-identity control rather than a
population.

## The ten components that already carry a scope

#1342 states that no closure member carries a native `scope`. Ten do.

```
pkg:github/nixos/nixpkgs@a799d3e...  scope=excluded  role=-              lifecycle=build
pkg:hackage/hspec@2.11.17            scope=excluded  role=artifact-input lifecycle=development
pkg:hackage/hspec-discover@2.11.17   scope=excluded  role=both           lifecycle=development
pkg:hackage/hspec-golden@0.2.2.0     scope=excluded  role=artifact-input lifecycle=development
pkg:hackage/HUnit@1.6.2.0            scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/QuickCheck@2.15.0.1      scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/call-stack@0.4.0         scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/haskell-lexer@1.2.1      scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/hspec-core@2.11.17       scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/hspec-expectations@0.8.4 scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
pkg:hackage/quickcheck-io@0.2.0      scope=excluded  role=artifact-input lifecycle=development  deriv=transitive
```

(Eleven rows; the nixpkgs component carries no closure role, so ten are
closure members.)

All are test frameworks, scoped by milestone 1269's propagation through the
**cabal** dependency graph. Their closure role is `artifact-input` because
`ARTIFACT_FIELDS` is `buildInputs`, `propagatedBuildInputs`, `depsHostHost`
(`closure/classify.rs:13`) and nixpkgs' Haskell builder passes test
dependencies through `buildInputs`.

### Why that changes the design

`artifact-input` means **"nix referenced it as an input to the thing being
built"**, not "it is present in the artifact at runtime". #1342's candidate
mapping reads row 1 as "it goes into the artifact; today's behaviour is
already correct", which is the stronger claim. `hspec` is the counterexample
on real data: `artifact-input` by role, and not in the shipped binary.

The consequence is a requirement rather than a preference: this feature may
only ever **add** a non-runtime scope. It must never assert `required`,
because for 131 of 231 `artifact-input` members it has no evidence for that.

## The compiler-supplied overlap

Milestone 1276 made a compiler-supplied package immune to *inheriting* a
non-runtime scope. This feature *assigns* one, so a boot library in a
tooling role would be in both populations, and invariant I9 asserts no
compiler-supplied component carries a non-runtime scope.

Candidates on the corpus — `pkg:hackage` components in a tooling or
unreferenced role:

```
pkg:hackage/happy@2.1.7   role=build-tooling   waybill:closure-identity=confirmed
```

Exactly one. Is it compiler-supplied? In the pinned revision's cached
configurations:

```
$ grep -n 'happy' ~/.cache/waybill/nixpkgs/a799d3e.../configuration-ghc-*.nix
configuration-ghc-9.4.x.nix:117:  happy = self.happy_1_20_1_1; # wants happy < 1.21
configuration-ghc-9.4.x.nix:123:  happy = self.happy_1_20_1_1; # wants happy < 1.21
```

`happy` appears only as an **alias**, never as `happy = null;`. `boot_set`
builds the boot set from `union_nulled` and the aliases from
`union_aliases` (`haskell_packages/mod.rs:1853-1855`), so `happy` is in the
alias map and not in the boot set.

**Measured overlap: zero.**

The rule FR-005 describes is still required — nothing prevents a nulled
package appearing in `nativeBuildInputs`, and I9 would fail if it did — but
it **cannot be verified on the corpus**. A corpus assertion would pass
whether the exemption works or not. Milestone 1275 found exactly this in its
own most-important test (findings F7 there: the fixture's collision was
protected by two independent rules, so the test could not fail), and the
answer was a fixture built to make the rule load-bearing. SC-007 says so
explicitly.

## The three counts the success criteria rest on

| | value | where |
|---|---|---|
| `build-tooling` members to be scoped | **80** | SC-001 |
| `artifact-input` + `both` members to be left alone | **265** | SC-002 |
| already-scoped members, all in roles this feature skips | **10** | SC-002 |
| corpus targets emitting no closure role | **16** | SC-004 |
| compiler-supplied ∩ tooling role | **0** | SC-007 |

| `unreferenced` members to be scoped | **17** | SC-001a |

Resolved in clarification (Session 2026-11-03): `unreferenced` carries
`excluded`, so the population this feature scopes is **97 of 362**.

One number is deliberately not in the table. Whether the role classifier's
seven field names cover the dependency keys real derivations carry is
**unmeasured** — every fixture in the tree carries only those seven, so
testing against them is circular, and a real closure needs a host `nix`.
FR-004a makes the enumeration a requirement and SC-009 asks for the count to
be *known*, not to be zero.

## T003 — the before-census, corpus-wide

Produced by the same instrument T049 will re-run after the feature
(`--census`), so the two outputs diff as a measurement rather than as two
hand-written tables.

```
| target                      | comps | roles | scoped | scope values | C203 | C203 values |
|-----------------------------|-------|-------|--------|--------------|------|-------------|
| go-cobra                    |     8 |     0 |      0 | -            |    0 | -           |
| go-opentelemetry            |   334 |     0 |     12 | {'excluded': 12} |    0 | -           |
| haskell-aeson               |    63 |     0 |     11 | {'excluded': 11} |    0 | -           |
| haskell-language-server     |   399 |     0 |     45 | {'excluded': 45} |    0 | -           |
| haskell-security-advisories |   251 |     0 |     75 | {'excluded': 75} |    0 | -           |
| image-postgres16            |   263 |     0 |      0 | -            |    0 | -           |
| maven-guice                 |    61 |     0 |     24 | {'excluded': 24} |    0 | -           |
| nix-closure-moat            |   387 |   362 |     11 | {'excluded': 11} |    0 | -           |
| npm-express                 |    45 |     0 |     16 | {'excluded': 16} |    0 | -           |
| pants-clojure-polyglot      |    55 |     0 |      0 | -            |    0 | -           |
| pants-example-django        |    47 |     0 |      0 | -            |    0 | -           |
| pants-example-golang        |     6 |     0 |      0 | -            |    0 | -           |
| pants-example-javascript    |   303 |     0 |    302 | {'excluded': 302} |    0 | -           |
| pants-example-jvm           |    29 |     0 |      0 | -            |    0 | -           |
| pants-example-python        |    12 |     0 |      0 | -            |    0 | -           |
| python-flask                |   108 |     0 |     73 | {'excluded': 73} |    0 | -           |
| rust-ripgrep                |    68 |     0 |      0 | -            |    0 | -           |

totals: 2439 components, 362 with a role, 569 scoped, 0 with C203, across 17 targets
```

Three things to take from it.

**569 scoped components corpus-wide reconciles with milestone 1276.** That
milestone measured 579 non-runtime scopes before its fix and removed exactly
10; 579 − 10 = 569. Two independently-derived numbers agreeing is the
cheapest confirmation available that this census reads the same field the
previous milestone did.

**`nix-closure-moat` shows 11 scoped, not the 10 quoted above.** The
eleventh is `pkg:github/nixos/nixpkgs`, which carries `lifecycle=build` and
no closure role, so it is not a closure member and no row of
`contracts/role-scope.md` applies to it. FR-008's byte-identity requirement
covers it: this feature must not touch it.

**Every scope in the corpus today is `excluded`.** No target emits
`optional` or an explicit `required`, so this feature introduces no new
value — only new components carrying an existing one. That matters for
FR-002: the schema's `"default": "required"` is the claim an absent field
already makes, and nothing in the corpus writes `required` explicitly.
