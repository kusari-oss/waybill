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
