# Data Model: Nix closure scope from the runtime closure

All of this is in-process state for one scan, except what is emitted (C203, C216 and the native marks).

## RuntimeClosure (closure tier, `closure/runtime.rs`)

| field | type | meaning |
|---|---|---|
| `paths` | `BTreeSet<String>` | store basenames that nix reports as the runtime closure of the outputs the evaluated attribute installs, falling back to its default output; C216 `outputs` names them (research R2, #1458) |
| `store` | `RuntimeStore` | `Local`, or `Substituter(url)`: the store that answered |
| `verified` | `bool` | signature-verified against `trusted-public-keys`; always true for `Local` |

Built only when a store answers completely and, for remote stores, verification passes. Otherwise the outcome is `RuntimeUnavailable(reason)`.

**`RuntimeUnavailable` reasons** form a closed enum with a wire string each:
- `not-in-any-store`
- `verification-failed`
- `query-timed-out`
- `no-evaluated-package` (the closure has no single evaluated root; `graph.evaluated` is `None`)
- `query-failed`

## StaticLinkSet (closure tier, `closure/static_link.rs`)

| field | type | meaning |
|---|---|---|
| `members` | `BTreeSet<DrvKey>` | Haskell library derivations linked into a runtime member, or into a Haskell build the evaluated wrapper root consumed (research R4, correction) |
| `source` | `StaticLinkSource` | `DeclaredEdges` (m1275, excludes tests) or `DerivationFields` (fallback, may include test deps) |

Go and Rust modules are not in this set. They inherit from their host through the existing `embedded-build-info` relationships.

## RuntimeEvidence (input to the scope pass)

`enum RuntimeEvidence { Closure { runtime: HashSet<MemberKey>, static_linked: HashSet<MemberKey> }, Walk { reached: HashSet<MemberKey> }, RoleWithLinked { linked: HashSet<MemberKey> }, None }`

`RoleWithLinked` is a walk set aside as `reached-only-root` (rule 4): the role rule decides, except for `linked`, the consumed Haskell builds plus their statically linked libraries.

`Closure.static_linked` is the static-link set seeded from the runtime members and, when the evaluated root is not itself a Haskell build, from the Haskell builds it consumed through `inputs.drvs` (`walk::consumed_haskell_builds`, which leaves out a build the root names only in a build-machine field), together with those seeded builds themselves when the runtime closure does not hold them. The runtime closure cannot be relied on to name the static build a wrapper's binary was copied from: aarch64-darwin's does, x86_64-linux's does not (`measurements/nix-behaviour/q10-*`).

Built in `scan_cmd.rs` (via `closure/runtime.rs::evidence`) from `ClassifiedClosure` before `assign_role_scope`, keyed by component identity (`MemberKey`) rather than `.drv`, because build variants share a component. `None` is used when the closure tier degraded, and today's behaviour is unchanged in that case; it is also used for `no-evaluated-package` (rule 5).

`Walk.reached` is the walk from the evaluated derivation over `ARTIFACT_FIELDS` (`closure/walk.rs`, the `reach.py` rule) united with the static-link set computed from the reached Haskell members. The `RuntimeUnavailable` reason is not carried here: the pass does not need it, and C216's `ScopeMethodRecord` holds it.

## ScopeEvidence (`waybill-common/src/resolution.rs`, beside `ArtifactPresence`)

`enum ScopeEvidence { RuntimeClosure, StaticallyLinked, ReachabilityWalk, ClosureRole }`

It records why a closure member got the presence it did. It is set on **every** closure member the pass decides, both marked and unmarked (research R6). The exemptions keep `ArtifactPresence::Exempt*` and no `ScopeEvidence`.

**Validation (and I10):** every component carrying `waybill:closure-role` ends the pass with either an exempt `ArtifactPresence` or a `ScopeEvidence`. A component whose evidence is `StaticallyLinked` never has a `NotInArtifact` presence. `RuntimeClosure` and `ReachabilityWalk` evidence appear on both marked and unmarked members (rule 3 and 4). Under runtime evidence the compiler-supplied exemption applies to any role, not only tooling roles: GHC boot libraries are linked from the compiler's own store path, which a static executable does not reference. A non-member embedded module is marked only when all its evidence lies in the Nix store; a module the project itself declares is never marked by the closure.

## Decision function (one place: `closure_role_scope::assign_role_scope`)

For each closure member `c`, the first rule that applies wins:

1. `c` is the evaluated package → `ExemptEvaluatedPackage`, as today.
2. `c` is a compiler-supplied boot library in a tooling role → `ExemptCompilerSupplied`, as today.
3. Under `RuntimeEvidence::Closure`:
   - `c`'s outputs intersect `runtime.paths` → runtime, `RuntimeClosure`;
   - else `c ∈ static_link.members`, or `c` is a Haskell build the evaluated wrapper root consumed through `inputs.drvs` (other than as a build tool), or `c` is an embedded module of a runtime host → runtime, `StaticallyLinked`;
   - else → `NotInArtifact`, `RuntimeClosure`.
4. Under `RuntimeEvidence::Walk`: `c ∈ reached` (or an embedded module of a reached host) → runtime, `ReachabilityWalk`; else `NotInArtifact`, `ReachabilityWalk`. A statically linked Haskell library is in `reached`, so it too carries `ReachabilityWalk`, never `StaticallyLinked`: its root is an approximation, and the document must not present it as proven (US2 scenario 2). The compiler-supplied exemption applies to any role, as under rule 3. A walk that reaches no member but the root, from a root that consumed a Haskell build through `inputs.drvs` (shellcheck's wrapper), is not used: `evidence` returns `RoleWithLinked`, so rule 5 applies, and C216 says `walk-outcome: reached-only-root`. The exception is the Haskell-builder derivation(s) the root consumed through `inputs.drvs`, other than one it names only as a build tool, and the libraries linked into them (the static-link logic, from those builds): they are runtime with `StaticallyLinked` evidence, whatever their role, because their code ships inside the wrapper's binary and the fallback errs toward runtime, never toward dropping linked code (T042). The compiler-supplied exemption still wins inside that set. Everything else stays on the role rule.
5. Under `RuntimeEvidence::None`: today's role rule, with `ClosureRole` evidence.

"Runtime" means `artifact_presence = None` (nothing asserted absent). Any `NotInArtifact` that the contents pass or the role rule set earlier is cleared in that case. That is FR-001 in both directions.

Lifecycle scope from milestone 1269 (e.g. `development`) is left untouched. Not-in-artifact and lifecycle are separate fields (`scan_cmd.rs:5106`).

## C203 `waybill:scope-classification-source` (extended closed enum)

| value | from |
|---|---|
| `closure-role` | today's rule (C216 method `closure-role`) |
| `closure-role+lifecycle-propagation` | as today |
| `exempt-compiler-supplied` | as today |
| `exempt-evaluated-package` | as today |
| `runtime-closure` | **new** |
| `statically-linked` | **new** |
| `reachability-walk` | **new** |

## C216 `waybill:closure-scope-method` (new document-scope row)

See `contracts/c216-closure-scope-method.md`. Built from `RuntimeEvidence` into `NixClosureSummary`. Emitted on every `--nix-closure` document whose closure tier ran; absent when the tier never ran.

## Edit selector

`Scope::NotInArtifact`, parsed from `excluded`. It matches:

| format | matches when |
|---|---|
| CDX | `scope == "excluded"` |
| SPDX 2.3 | the package is the source of a `BUILD_TOOL_OF` relationship |
| SPDX 3 | the element is the `to` of a `usesTool` `LifecycleScopedRelationship` with `scope: "build"` |

The CDX fallback that read `excluded` as `Development` is removed.
