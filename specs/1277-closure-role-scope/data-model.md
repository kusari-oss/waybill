# Data model — closure-role-derived scope

All state is in-process for one scan. Nothing is persisted, cached or read
back.

## The new per-component signal

A closed enum on `waybill_common::resolution::ResolvedComponent`, carrying
what the closure role established about the component's presence in the
artifact. Named for the fact, not for the field it happens to drive, because
the three formats realise it differently and two of them have no `scope`
field at all.

| state | meaning | set when |
|---|---|---|
| absent (`None`) | the closure role established nothing | no `--nix-closure`, a degraded closure tier, an absent role, a role of `artifact-input` or `both`, or a compiler-supplied member |
| `NotInArtifact` | nix referenced the member only in ways that do not put it in the artifact | the unioned C182 role is `build-tooling` or `unreferenced`, **and** the member is not compiler-supplied |

Two states, not four. The enum deliberately does **not** mirror
`DerivationRole`: `artifact-input` and `both` produce no state at all,
because FR-002 forbids this feature from asserting `required` — `hspec` is
`artifact-input` and does not ship. The role itself remains available in
full, as C182, for anyone who needs the four-way distinction.

**Exhaustive match required.** Per `feedback_enum_allowlist_falls_out_of_date`
and #892 — a `matches!(x, A|B|C)` allowlist silently missed a variant added
later and survived about two years — every consumer matches all variants
rather than testing membership of a list.

### Why not an existing field

| candidate | why not |
|---|---|
| `lifecycle_scope = Build` | Gated on `--include-dev` for the CDX emission, so the feature would not fire by default; and `--exclude-scope build` would then **drop** all 97 components, destroying the reason the `unreferenced` ones are emitted (research R1). |
| `build_inclusion = NotNeeded` | Carries `go mod why`'s specific semantic and its own catalog row. Reusing it would make one field mean two things in two ecosystems. Its *shape* is the model (research R2); its value is not. |
| the C182 annotation alone, read at emit time | The exemption needs the boot set, which lives in the resolve stage. Reading it at emit time would need a second source for "compiler-supplied" — and I9 already warns that two sources for that fact can each be locally satisfiable while the pair is not (research R3). |

## Inputs to the pass

| input | where it already is | note |
|---|---|---|
| the unioned closure role | `ResolvedComponent.extra_annotations["waybill:closure-role"]` | `orphan_reason.rs:392` reads this key today, so it is present at the resolve stage |
| the compiler-supplied set | threaded to the resolve stage by m1276 as `boot_libraries: Option<BTreeSet<String>>` | `None` exempts nothing — unearned exclusion is as dangerous as m1276's unearned immunity |

Both are in one place at one time, beside `propagate_non_runtime_scope`.
Nothing new is plumbed.

## The provenance record (FR-009)

A per-component annotation naming which mechanism classified the component
as non-runtime. Three values and an absence:

| value | meaning |
|---|---|
| `closure-role` | this feature, from nix's own dependency fields |
| `lifecycle-propagation` | milestone 1269, from the dependency graph |
| `both` | the component is in both populations; measured today as the empty set, since the 10 already-scoped members are all in roles this feature skips |
| *absent* | nothing set a scope. **Absence means "nothing set it", not "set by default"** — the distinction m1276 learned to make explicit when `{"compiler-supplied":0}` and no record at all turned out to be different facts |

It records the provenance of the **classification**, not "which mechanism
wrote the CycloneDX `scope` field". The second phrasing names a field SPDX
2.3 and SPDX 3 do not have; the first is a coherent claim in all three
formats (research R5).

## What this model does not change

- `DerivationRole` and its union rule are untouched. The scope is derived from the **unioned** role, never from one variant — measured, 13 of 97 duplicated identities in a real closure disagree about their role, and `flex 2.6.4` is `Both` as one variant and `Unreferenced` as another.
- `LifecycleScope` gains no variant, and the m1269 propagation and m1276 immunity are unmodified. Two mechanisms writing one field is what made #1342's premise wrong about ten components; this keeps them separable.
- Invariant I9 is **unamended** (FR-005, SC-005). It reads the emitted document, so it catches this feature getting the exemption wrong without being told the feature exists.
