# Edge accounting on the pinned moat closure — 2026-11-13 (#1367)

#1367 asked why the closure contributes fewer dependency edges than a
prediction made on 2026-11-05. That prediction mapped every `inputs.drvs` edge
to component identities and stopped there. The suspected cause was FR-013's
projection, which walks *through* scaffolding and fragment members and stops
at sources and packages. It had not been measured.

**Answer: the projection accounts for the whole difference.** Every
identity edge that the collapse has and the document lacks comes from walking
through a scaffolding or fragment member. No edge is lost for any other
reason.

## Input

The x86_64-linux closure that the corpus goldens pin was captured in CI by the
reader's own argv. It was run against a copy of the pinned clone, never
evaluated locally:

```sh
nix derivation show -r --option allow-import-from-derivation false \
    "<copy of the pinned clone>#packages.x86_64-linux.default" > moat-closure.json
```

| | |
|---|---|
| target | `nix-closure-moat` @ `d06905558ba68b49b17f36c875e97f7234a9c19b` |
| `flake.lock` sha256 | `139301fe846b8c75ec616ec63ab14f5bf4127d5b01851e3129c5feddb0c402c6` (unchanged from the local cache's copy) |
| closure sha256 | `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab` |
| format version, derivations | 4, 1,749 (the golden's C184 says 1,749) |
| CI run | public-corpus `workflow_dispatch` 43535531153, branch `1367-closure-edge-count` |

## Probe

`s1367_edge_accounting_over_a_real_closure` in
`waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`:

```sh
WAYBILL_TEST_CLOSURE_JSON=moat-closure.json \
  cargo test --bin waybill s1367_edge_accounting -- --nocapture
```

It calls the reader's own `project`, `emitted_drv_keys`, `stops` and
`key_of_derivation`. It does not re-implement them. Its control,
`s1367_the_buckets_fire_on_a_control_closure`, runs a constructed closure in
which each possible bucket fires. If `stops` is mutated to stop at everything,
that control fails.

Three runs gave byte-identical output.

## Result

| | edges |
|---|---|
| `inputs.drvs`, derivation level | **10,342** |
| ↳ from a member that is not an edge endpoint (`from-unemitted`) | 2,195 |
| ↳ into a member the walk passes through (`walked-through`) | 802 |
| ↳ into a stopping member that is not emitted (`to-unemitted-stop`) | **0** |
| ↳ kept as recorded (`direct`) | 7,345 |
| collapsed to identities, no projection (the prediction's quantity) | **9,661** |
| projected, derivation level | 7,761 |
| projected, identity level | **6,875** |
| ↳ also in the collapse | 6,689 |
| ↳ added by walking through (nix recorded a two-step path) | 186 |
| collapsed edges not projected | **2,972** |
| ↳ produced only by `from-unemitted` edges | 2,193 |
| ↳ produced only by `walked-through` edges | 779 |
| ↳ produced by anything else | **0** |

The figures reconcile:

- **9,661 − 2,972 + 186 = 6,875.** That equals C184 `edges` in the committed
  golden. Converting member identities to document PURLs therefore merges
  nothing further on this closure, and the probe reproduces the binary's count
  rather than approximating it.
- **The 2,195 `from-unemitted` edges start at 512 distinct members, none of
  which stops the walk.** 512 is also C184 `edge-ineligible-components` in the
  golden: the 267 scaffolding and 245 fragment members. These are walked
  through by design. None is a package that the projection failed to emit.
- **No collapsed edge is lost from a `direct` edge, and none is produced by two
  buckets at once**: the "only" and "any" counts are equal in every row.

## `to-unemitted-stop` is structurally empty

Milestone 1280 measured five residual orphans. Each had only consumers that
declare a `pname` and were not emitted (`stdenv-linux`, `patch`, `build`).
That population no longer exists.

Milestone 1372 gives every derivation that another derivation's `inputs.drvs`
names at least `BuildTooling`, and that role emits the derivation. An edge
target is named there by definition. A stopping target that is not emitted
would need `pname()` to return nothing, which takes a derivation with neither
a `pname` nor a `name`. The control asserts the zero.

## What this does not measure

#1367 quoted aarch64-darwin figures at `bf5fb15`: 5,131 edges against a
predicted ~6,479. The darwin closure was not re-captured, because evaluating
the corpus flake locally is out of bounds. Those figures predate milestones
1372 and 1377 and spec 1380 (#1374), all of which landed after `bf5fb15`.

The question was whether the projection explains the gap. That is a property
of the rule, not of the system, and on the closure the goldens pin the answer
is complete. *Inference, not measured: a darwin capture would show the same
partition with different magnitudes.*

## Conclusion for #1367

The edge count against the collapse is FR-013 working as specified. The
prediction was computed without the projection. Nothing drops edges.

The second figure in #1367 was C184's 5,658 against the document's 5,131. That
was settled by #1445, which made C184 count only closure-contributed edges
present in the document. On the current golden C184 reads 6,875 and the
document carries 7,347 `dependsOn` entries. The document is larger by the
edges other sources contribute, as #1445's doc comment states.
