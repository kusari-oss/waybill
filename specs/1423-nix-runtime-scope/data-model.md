# Data Model: Nix closure scope from the runtime closure

All of this is in-process state for one scan, except what is emitted (C203, C216 and the native marks).

## RuntimeClosure (closure tier, `closure/runtime.rs`)

| field | type | meaning |
|---|---|---|
| `paths` | `BTreeSet<String>` | store basenames that nix reports as the runtime closure of the evaluated installable |
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
| `members` | `BTreeSet<DrvKey>` | Haskell library derivations linked into a runtime member |
| `source` | `StaticLinkSource` | `DeclaredEdges` (m1275, excludes tests) or `DerivationFields` (fallback, may include test deps) |

Go and Rust modules are not in this set. They inherit from their host through the existing `embedded-build-info` relationships.

## RuntimeEvidence (input to the scope pass)

`enum RuntimeEvidence { Closure { runtime: RuntimeClosure, static_link: StaticLinkSet }, Walk { reached: BTreeSet<DrvKey>, reason: RuntimeUnavailable }, None }`

Built in `scan_cmd.rs` from `ClassifiedClosure` before `assign_role_scope`. `None` is used when the closure tier degraded, and today's behaviour is unchanged in that case.

## ScopeEvidence (`waybill-common/src/resolution.rs`, beside `ArtifactPresence`)

`enum ScopeEvidence { RuntimeClosure, StaticallyLinked, ReachabilityWalk, ClosureRole }`

It records why a closure member got the presence it did. It is set on **every** closure member the pass decides, both marked and unmarked (research R6). The exemptions keep `ArtifactPresence::Exempt*` and no `ScopeEvidence`.

**Validation (and I10):** every component carrying `waybill:closure-role` ends the pass with either an exempt `ArtifactPresence` or a `ScopeEvidence`. A component whose evidence is `RuntimeClosure` or `StaticallyLinked` has no `NotInArtifact` presence.

## Decision function (one place: `closure_role_scope::assign_role_scope`)

For each closure member `c`, the first rule that applies wins:

1. `c` is the evaluated package → `ExemptEvaluatedPackage`, as today.
2. `c` is a compiler-supplied boot library in a tooling role → `ExemptCompilerSupplied`, as today.
3. Under `RuntimeEvidence::Closure`:
   - `c`'s outputs intersect `runtime.paths` → runtime, `RuntimeClosure`;
   - else `c ∈ static_link.members`, or `c` is an embedded module of a runtime host → runtime, `StaticallyLinked`;
   - else → `NotInArtifact`, `RuntimeClosure`.
4. Under `RuntimeEvidence::Walk`: `c ∈ reached` (or an embedded module of a reached host) → runtime, `ReachabilityWalk`; else `NotInArtifact`, `ReachabilityWalk`.
5. Under `RuntimeEvidence::None`: today's role rule, with `ClosureRole` evidence.

"Runtime" means `artifact_presence = None` (nothing asserted absent). Any `NotInArtifact` that the contents pass or the role rule set earlier is cleared in that case. That is FR-001 in both directions.

Lifecycle scope from milestone 1269 (e.g. `development`) is left untouched. Not-in-artifact and lifecycle are separate fields (`scan_cmd.rs:5106`).

## C203 `waybill:scope-classification-source` (extended closed enum)

| value | from |
|---|---|
| `closure-role` | today's rule (method `none`) |
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
