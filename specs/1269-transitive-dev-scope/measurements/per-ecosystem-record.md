# Per-ecosystem outcome record — feature 1269 (#1209)

**FR-015** requires an outcome per ecosystem — *fixed* / *already
correct* / *not reached* — each citing a corpus target or a named
synthetic fixture. **SC-005** requires that no row read "assumed".

**No row is pending.** Three were, until corpus-lane run #40251099255
closed them — and closing them changed the feature rather than just
recording it: the lane found that the pass had been placed before the
nixpkgs component producers, so `cabal` and `nix` read as *not reached*
when they were in fact *fixed* by a correctly placed pass. The
account is in `measurements/t038-lane-and-placement.md`.

## Outcomes

| ecosystem | outcome | evidence | what the evidence shows |
|---|---|---|---|
| pypi / uv (PEP 735 `[dependency-groups]`) | **fixed** | synthetic fixture `waybill-cli/tests/fixtures/scope_propagation/uv`, test `uv_transitive_of_a_pep_735_dependency_group_is_marked_by_the_pass` | the dev-group member is `optional` from the uv reader; its exclusive transitive is `optional` **with** `waybill:lifecycle-scope-derivation = transitive`; the runtime branch in the same document is unmoved. This is #1209's `pydantic` shape. |
| pypi / pip (requirements + `pyproject`) | **fixed** | corpus target `python-flask`, `measurements/t024-before-after.md` | invariant I3 violations 51 → 0; marked components 22 → 77; 55 components gained a C62 `transitive` marker. Measured with the `HEAD~1` and `HEAD` binaries on the same input. |
| npm | **already correct** | corpus target `npm-express`, `measurements/t024-before-after.md`; FR-013's named control | 370 components, 303 marked, 0 I3 violations, 0 C62 markers — identical before and after. The npm reader already marks transitives of `devDependencies`, so the pass finds nothing unscoped to mark. |
| gem (Bundler groups) | **already correct** | synthetic fixture `.../scope_propagation/gem`, test `gem_transitive_of_a_development_group_is_already_scoped_by_the_reader` | the transitive of a `group :development` gem is `development` with **no** C62 marker — the gem reader classifies every spec outside the production BFS closure as non-runtime, transitives included (`gem.rs`, the `None => Development` arm). The edge the pass would have walked exists, which is what distinguishes *already correct* from *not reached*. |
| composer (`require-dev`) | **already correct** | synthetic fixture `.../scope_propagation/composer`, test `composer_transitive_in_packages_dev_is_already_scoped_by_the_reader` | `composer.lock`'s `packages-dev[]` is the full dev closure composer itself resolved, so the transitive is listed there beside its parent and `emit_lockfile_packages` scopes both. No C62 marker. |
| cargo | **not reached** — reader conflict, **#1272** | corpus target `rust-ripgrep`, `measurements/t024-before-after.md` | I3 violations 2 → 2. `pkg:cargo/ryu@1.0.18` and `pkg:cargo/itoa@1.0.11` are reached by Phase B and are not immune, but the cargo reader already assigned them `Runtime` while scoping their sole dependant `serde_json` `optional`. FR-003 forbids overwriting a reader's scope, so the pass declines. Filed as #1272; `rust-ripgrep`'s I3 declaration is retained and re-pointed there (FR-014). Cargo's *declared* dev-dependencies were already correct before this feature — what is unreached is the transitive closure of a **non-activated optional** dependency. |
| mix (Elixir) | **not reached** — missing edges | synthetic fixture `.../scope_propagation/mix`, test `mix_transitive_is_not_reached_because_the_reader_emits_no_edges` | the mix reader *does* read `only: [:dev, :test]` for a declared dep (the member comes out `development`), but constructs every `mix.lock` entry with `depends: Vec::new()`. `mix.lock` records hashes, not a graph. The transitive therefore has **zero inbound edges** and the pass has no non-runtime path to follow. Upstream of this feature. |
| rebar (Erlang) | **not reached** — profile lost at the lock tier | synthetic fixture `.../scope_propagation/rebar`, test `rebar_transitive_is_not_reached_because_the_lock_tier_loses_the_profile` | two independent blockers. (1) `erlang.rs`'s profile classifier (`Some("dev") \| Some("test") \| Some("doc")`) runs only on the **design-tier** path — deps with no `rebar.lock` match — so a `test`-profile dep that *is* locked comes out `Runtime` and there is no non-runtime seed in the document at all. (2) Lock entries are built with `depends: Vec::new()`, so there are no transitive edges either. Both upstream of this feature. |
| maven / gradle (`optional`, `provided`, test scope) | **already correct** | corpus target `maven-guice`, lane run #40251099255 | I3 passes (`.`) and all three goldens are **byte-identical**. Both halves matter: the invariant still holds, and the pass found nothing to mark, so the maven reader was already scoping transitives. |
| cabal / Haskell | **fixed** | corpus targets `haskell-language-server` and `haskell-security-advisories`, lane run #40251099255; `measurements/t038-lane-and-placement.md` | I3 26 → **0** and 13 → **0**; 26 and 72 components marked `development` with the `transitive` marker. `haskell-aeson` passes I3 and is byte-identical. These were *not* fixed by the first placement of the pass — the nixpkgs-haskell closure resolver produces their components after the reconcilers, so the pass ran before they existed. The earlier "62 components, not measurable on this host" note was wrong: that came from an invocation missing `--root-name`/`--root-version`, and with the harness's own invocation this host emits 251, matching the golden. |
| nix | **fixed** | corpus target `nix-closure-moat`, lane run #40251099255 | I3 9 → **0**, 9 components marked `development` with the `transitive` marker. Same placement cause and same correction as cabal. |

## Why five claims rest on synthetic fixtures

The corpus has 17 targets — `go-cobra`, `go-opentelemetry`,
`haskell-aeson`, `haskell-language-server`,
`haskell-security-advisories`, `image-postgres16`, `maven-guice`,
`nix-closure-moat`, `npm-express`, `pants-clojure-polyglot`,
`pants-example-django`, `pants-example-golang`,
`pants-example-javascript`, `pants-example-jvm`,
`pants-example-python`, `python-flask`, `rust-ripgrep` — and **none of
them is a gem, composer, mix, rebar or uv project**. US4 acceptance
scenario 2 requires that a claim about such an ecosystem say so rather
than imply corpus coverage, which is what the fixture column does.

A synthetic fixture shows that a code path fires on the shape the
fixture has. It does not show that projects in the wild have that
shape. For the three *not reached* rows that limit does not weaken the
finding — a missing edge is missing for every project, not just this
one — but for the two *already correct* rows it does: they establish
that the reader handles the common group/`require-dev` form, not that
it handles every form.

## The mix and rebar tests assert the gap deliberately

They are characterization tests. Closing either gap upstream makes them
**fail**, with a message saying this record is stale. That is the point:
the alternative is a record that silently stops being true, which is the
failure mode `specs/194-close-orphan-gaps/spec.md:60` produced when it
deferred this same class with *"filed separately if they surface"*.

Neither gap is filed yet. Both are reader defects of the same family as
#1272 and are candidates for the same treatment — recorded here rather
than filed unilaterally.
