# T038 — what the corpus lane found, and the placement defect it exposed

**Date**: 2026-10-26 | **Feature**: `1269-transitive-dev-scope` (#1209)
**Run**: `public-corpus.yml` #40249731861, `-f branch=1269-transitive-dev-scope -f regen_goldens=true`

## The lane's result

`106 passed, 3 failed`. `python-flask` and `rust-ripgrep` passed, which
confirms both halves of the amended SC-002: flask's declaration is
correctly deleted, and ripgrep's retained declaration is correctly
still declared.

The three failures were `haskell-language-server` (26),
`haskell-security-advisories` (13) and `nix-closure-moat` (9) — the
same counts as their deleted declarations, all `pkg:hackage/*`
violators.

The job name read `Run public corpus (1269-transitive-dev-scope)` and
the checkout step `ref: 1269-transitive-dev-scope`, which is the
documented check that `-f branch=` took effect rather than the lane
silently scanning main.

> Incidental: the run summary's `Commit:` line read `f8ff263f`, which is
> **main's** tip, not the branch's. It is `github.sha` — the ref the
> workflow file came from — while the checkout used the `branch` input.
> Cosmetic here, misleading in general, and worth a separate issue.

## Diagnosis: the pass was placed before a component producer

**This is a defect in this feature, not in a reader.** The three
targets were reported to the user as "pending, cause unknown"; the
cause is mine.

The call site was chosen on research R1's constraint that "the
component set must be final", verified against `deduplicate()` and both
m191 reconciler passes — all of which precede it. It was **not**
verified against component *producers* downstream of it. Two exist:

```
waybill::resolve::scope_propagation: propagation assigned nothing  immune=117 seeded_from=19
...
waybill::scan_fs::package_db::nix::haskell_packages: nixpkgs-haskell: runtime closure resolved
                                                    declared=128 transitive=232
waybill::cli::scan_cmd: nixpkgs-haskell: closure dependency edges emitted  closure_edges=2126
```

The nixpkgs-haskell runtime-closure resolver and the m1035 nix-closure
tier add 232 components and 2,126 edges to `haskell-language-server`
*after* the pass ran — including every one of its 26 violators. The
pass was operating on a set that was final only for ecosystems with no
post-reconciler producer, which is exactly why `python-flask` worked
and all three hackage targets silently did not.

### How it was found, because the first two hypotheses were wrong

1. **"FR-012 declined — hackage has no root."** Plausible: all three
   failing targets carry `transitive-edges-unresolvable: hackage` in
   their graph-completeness reason and `python-flask` does not. Wrong
   twice over — that reason code is m177's *design-tier* classifier,
   not a statement about roots, and the pass reported
   `rooted_ecosystems=["hackage"]`.
2. **"FR-003 declined — the reader stamped `Runtime`,"** i.e.
   `rust-ripgrep`'s shape. The haskell reader *does* construct every
   freeze- and stack-derived component with
   `lifecycle_scope: Some(Runtime)` (`haskell.rs`, three sites), which
   made this look right. Wrong: `HUnit` carries
   `waybill:nixpkgs-component-origin` and comes from the nixpkgs
   resolver, whose construction sites are `lifecycle_scope: None`.

Both were refuted by one log line, which did not exist until this
investigation needed it. The pass used to return early and silently
when it assigned nothing, so "the feature is a no-op here" and "the
feature declined here, deliberately, for a stated reason" were
indistinguishable from outside. It now logs on that path too, with
`skipped_unrooted_ecosystem` and `rooted_ecosystems`, and the summary
carries both. SC-001's exception (a) requires naming any component
skipped for want of a root, so the pass has to be able to say which.

## After relocating, measured on the cached corpus checkouts

Same binary, same inputs, harness-matching invocation
(`--root-name <target> --root-version <7 hex>`):

| target | assigned | I3 before | I3 after | declared count |
|---|---|---|---|---|
| `haskell-language-server` | 26 `development` | 26 | **0** | 26 ✓ |
| `haskell-security-advisories` | 72 `development` | 13 | **0** | 13 |
| `nix-closure-moat` | 9 `development` | 9 | **0** | 9 ✓ |
| `python-flask` | 51 `optional` | 51 | **0** | 51 ✓ |
| `npm-express` | nothing | 0 | **0** | — (FR-013 control) |
| `rust-ripgrep` | nothing | 2 | **2** | 2 — #1272 |

`haskell-security-advisories` assigns 72 against an I3 count of 13
because the pass marks population B as well, and I3 only counts
population A; the I3 count is what reaches zero.

**Four of the five declared targets are fixed.** Only `rust-ripgrep`
remains, as #1272, which is what the amended SC-002 says.

### A correction to an earlier claim

`haskell-security-advisories` emitted 251 components here, matching its
golden exactly. An earlier note in this feature said it emitted 62 and
was therefore "not measurable on this host", and the per-ecosystem
record's three pending rows rested on that. The 62 came from an
invocation without `--root-name` / `--root-version`. The host was never
the problem — `feedback_match_harness_invocation` says exactly this and
it was not applied.

## Open: the `--exclude-scope` ordering

The relocated call now sits **below** the `--exclude-scope` filter, so
FR-002's constraint 2 no longer holds: an operator excluding
`development` still receives the transitive closure of a declared dev
member. The constraint and the component-set constraint cannot both be
satisfied at the current filter position, because the producers are
below the filter.

Resolving it means moving the filter below the pass, which also makes
it apply to the nixpkgs and closure components it cannot currently see
— a change to an operator-visible flag beyond this feature's scope.
Left as-is and documented at the call site rather than widened
silently. The filter's own comment claims it is "the final step before
serialization", which has not been true since the nixpkgs tiers landed.
