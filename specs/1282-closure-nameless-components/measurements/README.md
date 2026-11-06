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
measurement **could not be run here.** It needs the pinned closure JSON, which
is produced by `nix derivation show --recursive` against the `nix-closure-moat`
flake; no capture is committed (it is ~6 MB of store paths) and this
environment has neither `nix` nor the flake. Only `results.txt` survives, and
it reports distinct/collision counts, not the hashes a prefix scan would need.

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
`nix-closure-moat` figures need `nix derivation show --recursive` against the
flake; this environment has neither. The corpus lane
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

### SC-008 — the mutation check (T040)

Disabling the new emission (`|| drv.declared_pname().is_none()` → `false &&`)
fails **14 of 19** integration tests. The 5 that still pass are correctly
insensitive, and each is explainable rather than merely surviving:

| test | why it is insensitive |
| --- | --- |
| `t038_a_scan_without_the_closure_flag_contributes_nothing` | asserts the tier does *not* run; must pass either way |
| `t035_the_c182_census_is_untouched` | counts derivations, which the emission cannot change (FR-012) |
| `t033_the_edge_figure_is_carried` | the key is present either way |
| `t049_two_scans_emit_identical_component_sets` | determinism holds over any set |
| `us3_no_edge_names_a_component_that_is_absent` | a safety property, vacuously true over a smaller set |
