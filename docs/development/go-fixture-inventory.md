# Go fixture inventory

What every Go fixture in this repository is for, who uses it, and
whether anything watches its output. Written so the next person editing
one does not have to re-derive this — the derivation took several wrong
turns, recorded at the bottom.

Established 2026-09-12 for [#843](https://github.com/kusari-oss/waybill/issues/843).

## The manifests

27 `go.mod` files under `waybill-cli/tests/fixtures/`. **20 declare
requires with no local `replace`**, which is what sends the toolchain to
`proxy.golang.org` on every scan of this tree.

| fixture group | manifests | consumer | intent |
|---|---|---|---|
| `golang/mod_why_scaling/{loose,mod-a,mod-b,mod-c}` | 4 | `tests/mod_why_scaling.rs` | incidental |
| `golden_inputs/golang/per_mainmod_scope_4modules` (+ `hack`, `tools`, `deep/src/thing`) | 4 | `tests/go_per_mainmod_scope.rs` | incidental |
| `golden_inputs/golang/workspace_mode/{module-a,module-b}` | 2 | `tests/golang_workspace_mode_preflight.rs` | incidental |
| `pants_go/*/3rdparty/go` and `pants_go/go_binary_first_party` | 8 | `tests/pants_go_reader.rs`, `tests/unresolved_reason_universal.rs` | incidental |
| `goroot_stub/app` | 1 | `tests/goroot_skip.rs` | incidental |
| `project_discovery/polyglot_nested_independent/services/worker` | 1 | `tests/project_discovery_scope.rs` | incidental |
| `split_modes/two_dir_polyglot/services/worker` | 1 | `tests/split_modes.rs` | incidental |

**No fixture is `deliberate`.** None of these consumers asserts that a
module fails to resolve; they assert on scaling, main-module selection,
workspace preflight, Pants target annotation, GOROOT skipping, project
discovery and split behaviour. US2's protected set is empty today. The
column stays because the rule needs somewhere to live when it is not.

## Golden coverage: none

**No committed golden covers any of these 20 manifests.** The
ecosystem-regression goldens under `waybill-cli/tests/fixtures/golden/`
take their inputs from the **sibling fixtures repository**, not from
this tree: `tests/common/mod.rs:137` resolves `fixture_path` against
`WAYBILL_FIXTURES_DIR`, which `build.rs` points at
`~/.cache/waybill/fixtures/<sha>/`. The `golang` case's input is
`go/simple-module` there, and it declares **real** modules
(`spf13/cobra`, `sirupsen/logrus`) that resolve from the local module
cache.

Consequence: editing the manifests above cannot churn a golden. Verify
it anyway — `git status` after a full test run — but do not design
around a risk that is not there.

## How to read this before editing a fixture

- **Adding a require?** Add a `replace` in the same commit. See
  [adding-go-fixtures.md](./adding-go-fixtures.md).
- **Changing what a fixture resolves to?** Check the consumer column
  first; those tests are the ones that will tell you if you were wrong.
- **Making a fixture deliberately unresolvable?** Point `replace` at a
  missing local path and change its intent here to `deliberate`. Do not
  leave the require unreplaced — that costs a network round-trip to
  achieve a failure you can have locally in 10ms.

## How this was derived, including the wrong turns

Recorded because three separate methods gave three wrong answers, and
the next person should not repeat them.

1. **Grepping for fixture names** returned ~25 consumer files, because
   `workspace_mode` is also a type name in this codebase. Useless.
2. **Grepping for full fixture paths** returned almost nothing, because
   tests build paths incrementally — `.join("per_mainmod_scope_4modules")`
   — so the full path never appears as a literal.
3. **Ancestor-prefix matching** returned 18 files per fixture by falling
   back to matching `golden_inputs`, which every golden test mentions.

What worked: grep for the fixture's **group directory** as it appears in
the test's `.join(...)` chain, then read the test to confirm. Slower,
correct.

A fourth wrong turn is worth recording too: the feature's own research
concluded "only three test files reference these fixtures". There are
seven consumer files. The undercount came from method 2.
