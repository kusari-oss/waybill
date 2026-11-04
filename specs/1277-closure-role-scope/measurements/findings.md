# Findings — milestone 1277

## F1 — every emitted `unreferenced` component is a patch-applier, by construction (T010)

`emit::emits_as_component` is

```rust
role_alone_emits(role) || drv.paths_in("patches").next().is_some()
```

and `role_alone_emits` excludes `Unreferenced`. So an `unreferenced`
derivation becomes a component **only if it applies a patch**. The 17 such
components on `nix-closure-moat` are therefore all patch-appliers — which
is exactly what #1342 reported ("unreferenced (patch-appliers) 22" on its
own fixture), now confirmed from the emission rule rather than from the
counts.

This strengthens FR-004's clarification. The component *is* the
patch-applying derivation; the patched code ships and the patch does not, so
`excluded` is the schema's own reading. It also meant the fixture needed a
`patches` entry on its `unreferenced` member, without which that member is
not emitted at all and FR-004 could not have been tested.

**And it sharpens FR-004a.** m1035 measured `jq` and `lua` in this role
carrying 6 of one project's 18 CVEs. Those are real runtime tools, and a
runtime tool landing in "referenced by neither field set" is precisely the
shape FR-004a asks about: a member referenced through a dependency key
waybill does not read. The probe matters more than it looked when the
requirement was written.

## F2 — the role filter is itself a `matches!` allowlist (T010, observation)

```rust
fn role_alone_emits(role: DerivationRole) -> bool {
    matches!(role, DerivationRole::ArtifactInput | DerivationRole::BuildTooling | DerivationRole::Both)
}
```

That is the #892 shape — a membership test over an enum, which silently
excludes a variant added later and survived about two years the last time.
Here the default is arguably safe (a new role would not emit, which is
fail-closed), and milestone 1277's own enum is matched exhaustively for this
reason. **Not changed**: it is outside this feature's scope and the
behaviour is defensible. Recorded so the next person adding a
`DerivationRole` knows the emission filter will not tell them.

## F3 — the fixture was measured before anything was asserted about it (T010, T016)

T016 requires the FR-005 fixture be built so the exemption is the only thing
preventing the marker, and names m1275's F7 as what carelessness costs
there. So the fixture's first test was a `#[ignore]`d diagnostic that prints
what the document actually holds, and the assertions were written from its
output:

```
name                               role             scope      purl
nixpkgs                            -                excluded   pkg:github/nixos/nixpkgs@a799d3e…
base                               -                -          pkg:hackage/base
waybill-fixture-roletest           artifact-input   excluded   pkg:hackage/…roletest@6.6.6
waybill-fixture-roletext           artifact-input   -          pkg:hackage/…roletext@2.0.2
waybill-fixture-roledual           both             -          pkg:hackage/…roledual@4.4.4
waybill-fixture-rolepatch          unreferenced     excluded   pkg:generic/…rolepatch@5.5.5
waybill-fixture-roletool           build-tooling    excluded   pkg:generic/…roletool@1.0.0
waybill-fixture-roletoolboot       build-tooling    -          pkg:hackage/…roletoolboot@2.2.2
waybill-fixture-roletoolhask       build-tooling    excluded   pkg:hackage/…roletoolhask@3.3.3
```

`roletoolhask` is what makes FR-005 testable: identical to `roletoolboot` in
role, ecosystem, GHC-built signal and closure position, differing only in
that the pinned revision's configuration does not null it. The control runs
both ways:

| mutant | result |
|---|---|
| exemption **deleted** | only `…the_exemption_fires_on_the_nulled_member_and_only_on_it` fails |
| exemption **widened** to every tooling-role member | that test **and** `…only_the_roles_that_do_not_ship_are_classified` fail |

Neither assertion can pass for a reason unrelated to FR-005, which is the
property m1275's F7 lacked.

The diagnostic is kept, `#[ignore]`d, because the next person to change this
fixture needs the same thing.

## F4 — one assertion was wrong and the measurement corrected it (T014)

FR-002's real shape is a member that is `artifact-input` by role **and**
already carries `excluded` from the m1269 propagation — the position ten real
components are in, `hspec` and its family. No member of the first fixture
draft was in it, so a `buildable: False` stanza was added to put one there.

The first assertion about it claimed `waybill:lifecycle-scope = development`,
by analogy with the real ten. It measured **`optional`**: a `buildable:
False` stanza yields m179's `Optional` variant, not `Development`. Corrected,
and the difference recorded in the test rather than papered over — FR-002 is
about preserving whatever the propagation set, not about which variant it
set, so the shape is reproduced either way.

## F5 — `pre-pr.sh` runs clippy with `-D warnings`, so a dead-code warning is a gate failure (T013)

`RoleScopeSummary::assigned()` was used only in tests, which clippy reports
as `method is never used`. Harmless-looking, and `scripts/pre-pr.sh` line 51
runs

```
cargo clippy --workspace --all-targets ${feature_args[*]:-} -- -D warnings
```

so it would have failed the gate. Fixed by logging the total in the INFO
line, which it should have carried anyway, rather than by an `#[allow]`.

## F6 — a grep-anchored edit found 57 of 58 sites, and missed the central one (T005)

175 `build_inclusion: None,` sites across 108 files, but that field is on
`PackageDbEntry` as well as `ResolvedComponent`, so a blanket substitution
would have been wrong in 118 places. A backwards-scan to the nearest
struct-literal opener got 57 insertions in 47 files with 118 correctly
skipped.

The one it could not see was the literal that never sets `build_inclusion`
at all — and that is the central `PackageDbEntry` → `ResolvedComponent`
construction at `scan_fs/mod.rs:992`, which every scanned component flows
through. `cargo build --all-targets` named it.

**The lesson is about the type, not the script.** Had `ResolvedComponent`
derived `Default`, nothing would have complained and the main construction
site would have stayed `None` silently — the feature would have worked in
every unit test and done nothing on a real scan. `closure_identity.rs:792`
already says why it does not derive it: "a component with every field
defaulted is a component nobody decided anything about."

## F7 — the propagation's provenance was already carried, which cut C203 down (T021, T023)

FR-009's first draft required the provenance recorded on **every**
non-runtime component. Measured against the catalog, that is redundant: C62
`waybill:lifecycle-scope-derivation` already records it, and `transitive` is
precisely "the m1269 propagation derived this".

Emitting a second annotation saying the same thing would have put a row on
every dev-scoped component in every ecosystem — 569 scoped components across
the corpus, by T003's census. That is the shape milestone 1276 measured at
**35 golden byte-identity failures across 11 ecosystems**, for a row that
outside Haskell could only ever say one thing.

So C203 is emitted only where the closure role concluded something, and the
spec's FR-009 carries the correction with the measurement behind it. The
regression suites confirm the gating: cdx/spdx/spdx3 11/11 each, zero
non-Nix goldens moved.

**SC-008 was wrong too, and for a related reason.** It asked that "every
scoped component names the mechanism that scoped it". Measured on
`nix-closure-moat`: 11 scoped components, **7 carry C62**, and four carry no
provenance at all — `nixpkgs`, `hspec`, `hspec-discover`, `hspec-golden`.
C62 fires only where waybill *derived* the scope rather than reading it from
a manifest, which is C62's existing rule and not this feature's business. The
criterion now asks what is achievable and says what the four are.

**A reuse option was considered and rejected.** C62's value set is an
open-enum naming how waybill derived a non-runtime scope
(`test-only-closure` / `go-mod-why` / `transitive`), and `closure-role`
would fit that vocabulary exactly. But C62 is C42's companion, and emitting
C42 means setting `lifecycle_scope` — which research R1 rejects on
measurement, because it puts the CycloneDX emission inside the
`--include-dev` gate and makes `--exclude-scope build` able to delete the
components. The rejection is recorded in C203's catalog row.

## F8 — two tests failed against correct code, and both were the test's fault

Worth recording together, because the shape is the same: an assertion
written from what a format was *assumed* to look like.

**The `optional`/`development` one** (F4 above) assumed a `buildable: False`
stanza yields `Development`. It yields m179's `Optional`.

**The envelope decoder** looked for a nested `waybill:annotation` object with
a `name` key. The real milestone-071 envelope is flat:

```json
{"schema":"waybill-annotation/v1","field":"waybill:…","value":"…"}
```

so the decoder returned `None` for every annotation and the cross-format
test failed against an emission that was already correct. Had it been
written the other way round — asserting absence — it would have **passed**
while reading nothing, which is the failure mode that costs a milestone
rather than five minutes.

Both were found by dumping the real document. `m1277_raw_spdx_annotation_shapes`
is kept, `#[ignore]`d, for the next person who needs to decode one of these.

## F9 — the bin/lib test-target split runs the opposite way for `parity`

The quickstart's note said `scan_fs` and `resolve` tests live in the bin
target, which is true. `parity` is declared in `lib.rs`, so its tests live in
the **lib** target, and `--bin waybill` reports `0 passed; 4571 filtered out`
for `extractors_table_is_sorted_by_row_id` — a test that exists and passes
under `--lib`.

The filtered-out count reads like a filter typo in both directions. The
quickstart now carries the table rather than the one-directional warning.

## F10 — the SPDX 3 vocabulary has a better answer than `dependsOn` (T046)

SPDX 3.0.1's `RelationshipType` has 59 values. The obvious reach is
`dependsOn` with `scope: "build"`, by analogy with what milestone 052
already emits for `BuildDependsOn` — and it would be wrong: it asserts a
dependency the artifact does not have. Enumerating the vocabulary instead
found **`usesTool`**, which says the artifact *employs* this element as a
tool.

So the two formats carry the same claim with the subject on opposite sides:

| format | construct | direction |
|---|---|---|
| SPDX 2.3 | `BUILD_TOOL_OF` | `member → root` |
| SPDX 3.0.1 | `usesTool` + `LifecycleScopedRelationship`, `scope: build` | `root → member` |

That reversal is not an inconsistency; it is the same one m228's convention
documents for the dependency types, because each vocabulary puts the subject
where its own wording reads correctly.

## F11 — the risk T048 was written for was already handled, by someone else's list (T048)

Adding 97 relationships to SPDX while adding none to CycloneDX's dependency
graph is the obvious way to break invariant I4, which asserts the two
formats agree on the subject's direct dependency set.

It does not break, because **`BUILD_TOOL_OF` is absent from the harness's
`SPDX_INVERSE_DEPENDENCY_TYPES`** (`invariants/checks/mod.rs:140`), so
`spdx_dependency_edges` ignores it. That list was written before this
feature existed and is correct now for the same reason it was then: a build
tool is not something the artifact depends on. Had `BUILD_TOOL_OF` been
included, SPDX would report 97 direct dependencies CycloneDX does not, and
I4 would fail on a pair of documents that are both right.

The local test transcribes the harness's normalisation rather than
re-implementing it. A re-implementation that happened to be laxer would pass
while the corpus failed, which is the inverse of a useful check.

## F12 — `-D warnings` caught two things `cargo test` was happy with, twice (T013, T046)

`scripts/pre-pr.sh:51` runs

```
cargo clippy --workspace --all-targets ${feature_args[*]:-} -- -D warnings
```

Both times the code compiled, all tests passed, and the gate would have
failed:

| | lint | cause |
|---|---|---|
| T013 | `method is never used` | `RoleScopeSummary::assigned` used only in tests |
| T046 | `redundant_closure`, `print_literal` | `.find_map(\|st\| envelope_field(st))` and a trailing `"purl"` argument to `println!` |

Neither is interesting in itself. The point is the interval: a green
`cargo test` says nothing about the gate, and running clippy only at the end
would have put both into a pushed branch. Worth running
`cargo clippy -p waybill --all-targets -- -D warnings` after each phase
rather than once at T041.

## F13 — writing the invariant forced a change to the feature's output (T026)

I10 could not be written as the spec described it, and the reason is the
most useful thing this milestone found.

The spec said the exemption "withholds the native scope; it does not
withhold the classification", and the implementation did exactly that: an
exempt member kept its C182 role and acquired nothing else. So in the
emitted document, a member the rule deliberately left alone and a member the
rule failed to reach are **the same component**. No invariant can tell them
apart, and neither can a reader.

The first attempt reused I9's signal — C170
`waybill:haskell-version-unresolved-reason == "compiler-supplied"` — and
measurement killed it:

```
--- waybill-fixture-roletoolboot ---
  scope = None
  waybill:sbom-tier = build
  waybill:closure-identity = confirmed
  waybill:closure-role = build-tooling
```

No C170. m1275 sets that property on the **synthetic** components it emits
for declared edges to boot libraries the closure does not contain; a closure
member that happens to be a boot library carries nothing of the kind. An
I10 reading C170 would have reported correct output as a defect.

So `ArtifactPresence` gained a third state, `ExemptCompilerSupplied`, and
C203 a third value, `exempt-compiler-supplied`. FR-005a was amended to
require it. This is milestone 1276's lesson arriving one level down: C202
has three states because "the rule applied and exempted nothing" and "the
rule did not apply" are different facts, and the same is true per component.

**The general shape**: writing the check is what revealed the output was
not auditable. A feature whose correctness nothing can observe is not
finished, however well it behaves.

## F14 — the first I10 would have failed on a correct refresh (T026)

`cdx_is_non_runtime`, the harness's shared helper, reads **only** C42
`waybill:lifecycle-scope`:

```rust
cdx_prop(component, "waybill:lifecycle-scope")
    .map(|s| NON_RUNTIME_SCOPES.contains(&s))
    .unwrap_or(false)
```

Milestone 1277's marker is the **native** CycloneDX `scope` field, set
deliberately without C42 so the emission stays outside the `--include-dev`
gate and `--exclude-scope build` cannot delete the components (R1–R2). So
the first draft of I10 would have reported 95 of the 97 classified
components as offenders *after* a correct refresh — passing today for the
wrong reason and failing tomorrow for a worse one.

Caught by predicting the row against the committed goldens before the
refresh rather than after:

| | population | already marked | offenders |
|---|---|---|---|
| committed goldens | 97 | 0 | **97** |

The 97 are the defect, which is the declaration's content. The fix is a
local predicate accepting either spelling, and the shared helper is
deliberately **not** widened: I3 uses it to define its own population, and
changing it would alter a different row's behaviour as a side effect of this
one. Milestone 1277 adds no C42, so I3 is unaffected either way.

## F15 — the declarations file sorts lexicographically, so I10 goes between I1 and I2

`sort_unstable()` over `(invariant, target)` string tuples, so `"I10"` <
`"I2"`. Appending the entry at the end failed three tests — the validity
check, the line-stability check and the golden-matrix check — with one
message about sort order. Worth knowing before adding `I11`.

## F16 — I lost clippy's exit status to a pipeline, and the absence read as success

Run as

```sh
cargo test ... | grep -E 'test result' | head -8 && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -E '^error' | head -6; echo CLIPPY_DONE
```

`head -8` closes the pipe, the left side of the `&&` exits non-zero, and
**clippy never runs**. My grep for `^error` then found nothing — because
there was no output at all — and the result looked exactly like a clean
lint.

Caught only because `CLIPPY_DONE` was absent from the log, which is the
positive signal the project's own gate discipline insists on: *assert on a
positive signal, never on absence*, because a check that did not run is
indistinguishable from one that passed. The same reasoning produced
`scripts/gate.sh`, which exists precisely so a caller who pipes the gate and
loses `$?` can still recover the verdict.

Recorded because the lesson is written down in `CLAUDE.md`, I had quoted it
earlier in this very milestone (F12), and I still made the mistake inside a
command whose purpose was verification. The fix is mechanical: run the gate
standalone, redirect to a log, and read its own exit status —
`cargo clippy ... > log 2>&1; echo "EXIT=$?"` — never chained behind a
pipeline whose status is something else's.

## F17 — FR-004a's answer is not zero: the role classifier reads 7 of about 20 dependency attributes (T031, T032)

Measured against a **real** closure, produced locally with the same command
waybill runs:

```sh
nix --extra-experimental-features 'nix-command flakes' \
    --option allow-import-from-derivation false \
    derivation show -r '.#packages.aarch64-darwin.default'
```

on the cached `nix-closure-moat` (`MercuryTechnologies/moat`) checkout —
1,275 derivations, 6.0 MB.

| | count |
|---|---|
| env keys carrying at least one store path | 104 |
| of those, nixpkgs **dependency attributes** | 6 |
| dependency attributes named by neither field list | **2** |
| members classified `Unreferenced` that are referenced through one | **6** |

The two unread attributes, and where each belongs:

| attribute | derivations | belongs in |
|---|---|---|
| `propagatedNativeBuildInputs` | 3 | `TOOLING_FIELDS` — propagated *native* inputs run on the build machine |
| `depsTargetTargetPropagated` | 7 | `ARTIFACT_FIELDS` — the propagated form of `depsTargetTarget` |

The six members: `bootstrap-stage0-compiler-rt`, `bootstrap-stage0-libcxx`,
`compiler-rt`, `compiler-rt-libc`, `libcxx`, `nss-cacert`. Two of them —
`compiler-rt` and `compiler-rt-libc` — carry a `patches` key, which is the
only way an `unreferenced` member is emitted at all
(`emit::emits_as_component`), so those two would receive FR-004's `excluded`
while in fact being referenced as dependencies. `nss-cacert` is the one that
reads worst on inspection: CA certificates are a runtime input, and the role
says nothing references them.

### Two caveats, both load-bearing

**The count is platform-specific; the incomplete lists are not.** This
evaluation is `aarch64-darwin` and the corpus goldens are produced on Linux
x86_64. None of the six appears in the committed golden, and `compiler-rt`,
`libcxx` and `nss-cacert` are LLVM/darwin-shaped. So **the corpus does not
currently exhibit this**, and that is a platform accident rather than a
guarantee. The CI figure is unknown and would need the same probe run there.

**The first version of this probe reported 287, and that number was
meaningless.** It counted every env key carrying a store path, which
includes `builder` (677 derivations), `buildCommand`, `checkPhase`,
`installPhase`, and the `bin` / `dev` / `doc` / `dist` keys naming a
multi-output derivation's *own* outputs. A patch file named in `patches` is
not a dependency the classifier missed. Restricting to the 20 real nixpkgs
dependency attributes took the answer from 287 to 6. Recorded because 287 is
exactly the kind of figure that reads as a measurement and is arithmetic over
an unexamined assumption.

### Scope

The lists read 7 names: `buildInputs`, `propagatedBuildInputs`,
`depsHostHost`, `nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost`,
`nativeCheckInputs`. nixpkgs' stdenv defines about 20. Beyond the two above,
`checkInputs`, `installCheckInputs`, `nativeInstallCheckInputs`,
`depsBuildTarget`, `depsHostTarget`, `depsTargetTarget` and the remaining
`*Propagated` forms are all unread — they simply do not appear in *this*
closure, so this probe cannot say whether they appear in others.

**SC-009 is satisfied by the number being known and recorded.** It is not
zero, which makes it a finding to scope rather than a detail to absorb: the
lists predate this milestone, the defect is in the classifier rather than in
anything 1277 added, and 1277 is what makes it consequential by attaching a
conformance claim to the role. Filed as **#1346**.

The probe is committed, `#[ignore]`d and env-gated, so the next person can
re-run it on another platform and on another closure:

```sh
WAYBILL_TEST_CLOSURE_JSON=/path/to/closure.json \
  cargo test -p waybill --bin waybill m1277_field_coverage -- --ignored --nocapture
```
