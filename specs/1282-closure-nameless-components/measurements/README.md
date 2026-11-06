# Measurements: the pname-less closure population

Run against the pinned `nix-closure-moat` closure (x86_64-linux, 1,749
derivations). `classify-nameless.py` is the probe; `results.txt` is its output
at the time of writing.

```sh
python3 -I classify-nameless.py <closure.json> --identity
```

Produce the input with `nix derivation show --recursive <flakeref>`.

## What it found

| | |
| --- | --- |
| closure derivations | 1,749 |
| with `pname` (emitted today) | 604 |
| **without `pname` (emitted today: nothing)** | **1,145 — 65%** |

Four classes among the 1,145:

| Class | Count | What they are |
| --- | --- | --- |
| `source` | 549 | fixed-output fetches; 138 carry a URL, 36 of those forge-hosted |
| `package` | 83 | version-bearing bootstrap packages (`mes-libc-mini-0.27.1`) |
| `scaffolding` | 267 | `writeText` output — `itoa-builder`, `setup-hook.sh` |
| `fragment` | 246 | live-bootstrap C functions — `strncpy`, `memcpy`, `malloc` |

## Identity

| Scheme | Distinct | Collisions |
| --- | --- | --- |
| name alone | 957 | **188 collide** |
| name + `.drv` hash | **1,145** | **0** |
| name + content hash (the 549) | 502 | 47, and they are correct merges |

`source` alone covers **56 distinct contents**, which is why a name-keyed
identity is not an option. The 47 "collisions" on the content-hashed path are
the same content fetched by two derivations, which is one component and not
two — so that path is a deduplicator, not a conflict.

Every one of the 1,145 can therefore be told apart: the `.drv` hash is nix's
own content-address of the build recipe, and it is present for every node
including the ones carrying no content hash of their own.

## Three probe errors, recorded because each produced a figure that looked right

1. **`builder` is a top-level key, not an `env` key.** Testing `env["builder"]`
   invented a class of 421 "derivations with no builder at all" that does not
   exist.
2. **A fixed-output derivation records its hash in `outputs[<name>]["hash"]`.**
   `env["outputHash"]` is the older spelling and appears on 139 of the 549 —
   reading only that undercounts the content-addressed population by a factor
   of four.
3. **The top-level `version` key is the derivation *format* version**, `"4"`
   for all 1,749 nodes. Read as a package version it makes every bootstrap
   fragment look versioned, including `strncpy`.

A fourth belief did not survive either: the forge-coordinate path, carried
into this work as the primary identity scheme, covers **36 of 1,145** — 3%.

## Not measured here

**aarch64-darwin.** Every figure above is x86_64-linux. The darwin closure is
a different closure, not a scaling of this one, and milestone 1280 made the
cross-system extrapolation error three times. Any darwin figure this feature
needs must come from a darwin capture.

## T010 — the hash-prefix length, and why there is no prefix

The task asked for a prefix length chosen by measurement: re-run the probe
over candidate prefixes and take the shortest with zero collisions. That
measurement **would not answer the question here.** It needs the pinned closure
JSON, which is produced by `nix derivation show --recursive` against the
`nix-closure-moat` flake; no capture is committed (it is ~6 MB of store paths),
and this host evaluates to `aarch64-darwin` while the goldens pin the
**x86_64-linux** closure. A local capture is a different closure, not a cheaper
view of the pinned one. Only `results.txt` survives from the original probe,
and it reports distinct/collision counts, not the hashes a prefix scan would
need.

**Decision: do not truncate.** The identity carries the full hash.

This is the one choice that needs no new measurement. R6 measured zero
collisions over **full** hashes — `name + .drv hash` gives 1,145 distinct of
1,145 — and that is precisely the claim the implementation now relies on.
Collision-freedom over a truncated prefix is a *different* claim; adopting a
conventional length and assuming it transfers is what the plan's "What this
plan deliberately does not settle" section exists to forbid.

The cost is a longer identifier: `pkg:generic/source-<32 base32 chars>` rather
than a short prefix. That is legibility paid for correctness, and FR-016
already keeps the human-readable name on the component's `name` field, so no
reader has to read the digest to learn what something is.

**What would reopen this**: a committed closure capture, or a prefix scan run
on a machine with the flake. `classify-nameless.py --identity` already prints
the per-scheme distinct counts; a prefix scan is a few lines on top of it. If
someone adds one, the bar is zero collisions at the chosen length **plus a
margin**, measured on both x86_64-linux and aarch64-darwin — they are
different closures, and milestone 1280 made the cross-system extrapolation
error three times.

## T039 / T041 / T042 — measured after implementation

**On the pinned closure: not measured here, and not predicted.** The
`nix-closure-moat` figures are the **x86_64-linux** closure the goldens pin;
this host is `aarch64-darwin`, so anything it evaluates is a different closure
and cannot stand in for them. The corpus lane
(`public-corpus.yml`) is where those numbers come from, and T045 refreshes the
goldens through it. Research R3's **−35% is a derivation-level proxy** and is
not the component-level result; reporting it as one is the error T039 exists
to forbid.

### What was measured: the fixture closure, scanned by the real binary

`waybill-cli/tests/fixtures/nix_closure_members/closure.json`, 11 derivations,
via the stub `nix` the integration suite uses. The C184 record as emitted:

| key | value |
| --- | --- |
| `derivations` | 11 |
| `components-emitted` | 9 |
| `components-without-member-role` | 1 |
| `member-roles` | `{source: 3, package: 1, scaffolding: 2, fragment: 2}` |
| `edge-ineligible-components` | 4 |
| `edges` | 1 |
| `roles` (C182, derivations) | `{artifact-input: 3, build-tooling: 1, unreferenced: 7}` |

FR-009 reconciles: `1 + (3+1+2+2) = 9` = `components-emitted`. The C182 census
still totals every derivation (11) and is not an addend.

9 pname-less members yield 8 components: the two `waybill-fixture-tarball`
members record the same content hash under the two different spellings and
merge, while the two members named `source` record different hashes and stay
apart. That is FR-003 and FR-002 in one fixture.

### Two defects this measurement found, which no test had caught

**1. The fixture produced zero edges, and a test passed over the empty set.**
The first fixture declared dependencies only in `inputs.drvs`. The C182
classifier reads *declared attributes*, so every derivation came back
`unreferenced`, nothing was emitted to hang an edge on, and
`m1282_us3_a_source_behind_scaffolding_is_the_endpoint` asserted "scaffolding
is never an edge endpoint" over a set with no edges in it. Fixed by declaring
the dependencies as attributes too; the test now carries a control asserting
the closure contributes at least one edge, and that control fails when the
declaration is removed.

**2. `source` and `package` orphans claimed the wrong reason.** The FR-014 arm
was keyed on "carries a member role", so an unreachable `source` — which *is*
an eligible endpoint under FR-013 — was told it was "not an edge endpoint".
That is a false statement in the slot a reader trusts to explain the orphan,
which is worse than the silence FR-014 removes. The arm now covers
`scaffolding` and `fragment` only; a `source` or `package` nothing reaches
falls through to `reachable-only-through-unemitted-package`, which on this
fixture is the true cause — the derivation consuming it was not emitted.

Both were found by dumping the emitted record and reading it, not by reading
the requirements. The suite was green before each.

### SC-008 — the mutation check (T040), re-run and then repaired

An earlier pass recorded "fails **14 of 19** integration tests". That did not
reproduce. Re-run at `7baebd40` with the tree otherwise clean, the single-site
mutation (`emits_as_component`'s `|| drv.declared_pname().is_none()` →
`|| (false && …)`) failed **7 of 19**.

**Why twelve survived.** Five are insensitive by design. One
(`us3_a_source_behind_scaffolding_is_the_endpoint`) asserts a property of
`graph::stops`, which the emission gate does not govern. The other **six were
insensitive by accident**: exactly three of the nine `pname`-less fixture
members were referenced by a *declared attribute* of another derivation —
`source` (`cccccccc`) and `waybill-fixture-tarball` (`ffffffff`) via
`buildInputs`, `waybill-fixture-builder` (`dddddddd`) via `nativeBuildInputs`
— so the C182 role gate admitted them with or without this milestone, and
those three were the members the six asserted on.

**A seventh defect, found while checking the sixth.** `us3`'s comment said
"`app` reaches `source`(B) only through `waybill-fixture-builder`". `app`
declares a `pname` and nothing references it, so the C182 gate **suppresses
it** — the closure contributed exactly one edge, `lib` to a tarball it names
directly, and no scaffolding stood between any emitted pair. The pass-through
scenario FR-013 exists for was not in the fixture at all, and the test did not
assert its own headline either: it checked only that the scaffolding was *not*
an edge target, never that the source behind it *was*.

#### What was changed

Fixture (`tests/fixtures/nix_closure_members/closure.json`), 11 → 12
derivations:

* **`waybill-fixture-edge-input` added** — a fixed-output member that exists
  to be the one `lib` names in `buildInputs`. That frees **both** tarballs to
  be unreferenced, which is the only way `identical_content_yields_one_component`
  can fail when the emission is off.
* **The source URL moved** from `source`(A), which `app` references, to
  `source`(B), which nothing declares.
* **`waybill-fixture-builder` became an input of `lib`**, which *is* emitted,
  so the walk finally has scaffolding to pass through:
  `lib → builder → source(B)` now projects to the edge `lib → source(B)`.
* `source`(A) and the builder stay attribute-referenced on purpose, as the
  "admitted by both paths" case.

Tests: four assertions retargeted onto unreferenced members, `us3` given the
half it never asserted, and `t055`/`t056` tightened from relations that hold
over any subset (`by_role > 0`, `unreachable > by_construction`) to pinned
figures. One unit-test count moved with the fixture (`source` derivations
4 → 5).

#### Measured after the repair

The mutation now fails **14 of 19**, and all seven targets are in the list by
name:

```
m1282_control_the_pname_less_members_are_emitted_at_all
m1282_t036_every_unreachable_added_component_carries_the_new_reason
m1282_t055_the_component_count_reconciles_without_the_census
m1282_t056_the_orphan_count_is_a_lower_bound_not_an_equality
m1282_us1_a_fixed_output_member_carries_its_content_hash_in_its_identity
m1282_us1_a_source_url_is_carried_natively
m1282_us1_identical_content_yields_one_component
m1282_us1_same_name_different_content_yields_two_distinct_components
m1282_us1_the_name_field_is_never_rewritten
m1282_us2_a_write_text_member_is_marked_scaffolding
m1282_us2_every_added_component_carries_exactly_one_role
m1282_us2_filtering_to_sources_and_packages_leaves_no_scaffolding
m1282_us2_the_role_is_carried_in_all_three_formats
m1282_us3_a_source_behind_scaffolding_is_the_endpoint
```

The five survivors are exactly the five insensitive by design:

| test | why it cannot be sensitive |
| --- | --- |
| `t038_a_scan_without_the_closure_flag_contributes_nothing` | asserts the tier does *not* run; must pass either way |
| `t035_the_c182_census_is_untouched` | counts derivations, which the emission cannot change (FR-012) |
| `t033_the_edge_figure_is_carried` | the key is present either way |
| `t049_two_scans_emit_identical_component_sets` | determinism holds over any set |
| `us3_no_edge_names_a_component_that_is_absent` | a safety property, vacuously true over a smaller set |

#### The edge control, proved rather than assumed

Changing which derivations are attribute-referenced is what made the edge set
empty once before, so the control was re-verified by a **second, independent**
mutation: deleting `app`'s `buildInputs` makes `lib` unreferenced and
therefore unemitted, and
`us3_a_source_behind_scaffolding_is_the_endpoint` fails with *"the closure
must contribute at least one edge, or this proves nothing"*. The control has
teeth.

#### A note on where these tests live

`cargo test -p waybill --lib m1282` reports **"0 tests; 218 filtered out"**.
The `scan_fs` tree is in the **bin** target, so the `member.rs` count that
moved with the fixture was invisible to `--lib` — a result indistinguishable
from a pass. `--bins` found it. This is the project's documented trap in its
other direction: `parity` lives in the lib, `scan_fs` in the bin.

#### A second variant, and why its number is not evidence

Mutating `is_1282_member` as well fails 18 of 19 — including
`t035_the_c182_census_is_untouched`, which emission cannot affect. A census
test failing means the scan itself broke, so that variant is a crash rather
than a clean mutation and its count says nothing about sensitivity. Only the
single-site figure is quoted.

## The pinned closure, measured (T043 / T045 / T046)

Source: `public-corpus.yml` run **42222424351**, dispatched against
`1282-closure-nameless-components` at `7baebd40`. The workflow's own
`Resolve the scanned commit` step reports the SHA it checked out, so this
is the commit that was scanned and not the one `github.sha` would have
named (#1287). Figures below were read out of the regenerated
`nix-closure-moat` goldens, not predicted.

### The C184 record, before and after

| key | before | after |
| --- | --- | --- |
| `derivations` | 1,749 | 1,749 |
| `components-emitted` | 381 | **1,463** |
| `edges` | 3,459 | **4,533** |
| `roles` (C182 census) | `{artifact-input: 268, both: 45, build-tooling: 121, unreferenced: 1315}` | **identical** |
| `patches` | 212 | 212 |
| `member-roles` | — | `{source: 502, scaffolding: 267, fragment: 245, package: 73}` |
| `components-without-member-role` | — | 376 |
| `edge-ineligible-components` | — | 512 |

Whole-document figures (the closure tier plus the Haskell reader):
**406 → 1,487 components, 3,748 → 4,644 edges.**

### Criterion by criterion

- **SC-001** — predicted "approximately 1,098"; measured **+1,081**
  net: 1,086 identifiers added, 5 removed (the re-identified five below).
  Every one of the 1,086 carries a member role, and 1,087 components carry
  one in all — the extra is `pkg:hackage/time`, which already existed and
  merely gained the annotation. The probe counted *derivations*;
  what is emitted is *components*, and the content-addressed path merges
  549 derivations into 502. `package` came in at 73 against a predicted 83
  and `fragment` at 245 against 246: the probe classified derivations,
  some of which merge.
- **SC-002** — `376 + (502 + 267 + 245 + 73) = 1,463` = `components-emitted`. Reconciles.
- **SC-003 / SC-014** — all 26 name-colliding groups have pairwise-distinct
  PURLs; **0 groups repeat a PURL**. See the I2 declaration.
- **SC-006** — 15 of 17 corpus targets regenerated **byte-identical**
  goldens. `image-postgres16` also changed; its entire delta is three
  upstream image/layer digests (`3ce18cb3…` → `38b0c486…` and two others),
  with no `waybill:closure-member-role` on either side. That is #1358's
  mutable-tag drift, so those goldens are deliberately **not** in this
  change.
- **SC-011** — edges re-measured: 3,748 → 4,644 whole-document. Not growth
  but **rewiring**: 344 edges removed, 1,240 added. `curl@8.20.0` lost 306
  in-edges and gained 355 — the walk now stops at the source that was
  previously walked through. Every edge resolves (SC-009 passed; the
  invariant lane reports no dangling reference).
- **SC-012** — 512 components are edge-ineligible (`scaffolding` + `fragment`);
  **510 carry the new reason** and the other **2 are reachable**, so they are
  not orphans and need none. Both are reached only by *hackage* in-edges from
  the Haskell reader, never by the closure projection, so FR-013 holds: no
  scaffolding or fragment is an endpoint of an edge this feature emits. The
  excess over the 513 the spec quoted is therefore −3 and fully accounted:
  one fewer fragment than the probe predicted, and two members the Haskell
  reader reaches by name.
- **SC-013** — the census is **numerically identical**, key for key.

### The five components whose identifier changed

Five PURLs present before are absent after:
`ghc-9.10.3-source`, `glibc-iconv-2.42`, `nuke-refs`, `remove-references-to`,
`uname`. None was lost. Each declared no `pname` but passed the old C182 role
gate, so it was emitted under a `(name, version)` key — and that key is R8c's
collapse, observed on real data:

| name | components before | after |
| --- | --- | --- |
| `nuke-refs` | 1 | **5** |
| `uname` | 1 | **2** |
| `glibc-iconv-2.42` | 1 | **2** |
| `ghc-9.10.3-source` | 1 | 1 |
| `remove-references-to` | 1 | 1 |

So five identifiers were rewritten and five derivations stopped being
merged into two components they were never the same artifact as. SC-007 is
about components *another reader* emits and is unaffected: no such component
was displaced or renamed, and 0 surviving components changed name or version.

### What this refresh carries that is not milestone 1282

The committed golden predates milestone 1281 (#1362), which landed without
refreshing it. So the `licenses` key in C184 (`received: 219, wanted: 1065`)
and the SPDX 3 growth in `simplelicensing_LicenseExpression` (8 → 32) are
**1281's**, not this feature's: of the 234 components carrying a license,
**233 carry no member role**. The one that does is
`expand-response-params`, a `fragment` whose nixpkgs `meta.license` declares
MIT — so the data model's "licenses: none for this population" was a
prediction the measurement narrowed to "one of 1,087", not a requirement.

### The one invariant cell that moved

`I2 / nix-closure-moat` — `duplicate-library-identity` ×26, newly declared.
`I3` stayed within its existing declaration and `I7` **passed**: every
unreachable component is explained, which is what R8b's narrowing was for.
`explained_pre_1156` was not touched.
