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
