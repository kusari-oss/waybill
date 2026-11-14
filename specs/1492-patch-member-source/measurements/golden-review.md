# Golden review (FR-008, SC-003, SC-004)

**Source.** public-corpus run 43547217059, `regen_goldens=true`, on branch
`1492-patch-member-source` at `dc1f7993`.

**Completeness.** The artifact `corpus-goldens-regen` holds 67 files, the
same set as `waybill-cli/tests/fixtures/public_corpus/`. No file is missing
in either direction.

**What differs.** 15 files: CycloneDX, SPDX 2.3 and SPDX 3 for each of the
five `nix-closure-*` targets. The other 52 are byte-identical (SC-004 for
every non-closure target).

## How each target was checked

Per target, the CycloneDX documents were compared component by component
(nested components included), edge by edge (`dependencies[].dependsOn`) and
document property by document property. The SPDX documents were checked with
`waybill sbom parity-check` on the old and new triples.

**The deduction for targets without a captured closure (C2).**
- Those goldens do not record `patches`.
- The new rung is the only new way into `source`.
- So every C206 change must be `fragment` → `source`, and every changed edge
  must touch such a member.
- Any other role change, or an unexplained edge, would be a defect.

## Result

| target | members moved `fragment` → `source` | edges Δ (doc) | C184 `edges` | `edge-ineligible` | orphans not reachable |
|---|---|---|---|---|---|
| moat | 5 | +16 / −0 | 6,875 → **6,891** | 512 → 507 | 513 → 508 |
| openai | 5 | +196 / −0 | 48,140 → 48,336 | 80 → 75 | 81 → 76 |
| shellcheck | 5 | +16 / −5 | 10,208 → 10,224 | 513 → 508 | (none reported) |
| slack-web | 6 | +235 / −0 | 44,191 → 44,426 | 80 → 74 | 81 → 75 |
| slacklinker | 6 | +20 / −0 | 12,149 → 12,169 | 523 → 517 | — |

On every target:

- **Roles.** Every changed component is a patch-named member moving
  `fragment` → `source`, and no other role changes. The members are
  `001-fix-rpath.patch`, `catalog-legacy-uris.patch` (×2),
  `fix-build-time-run-tcl.patch`, and `remove-hatch-plugins.patch`,
  `version.patch` or `locale-binary-path.patch` depending on the closure.
- **Properties.** The only component properties that change are
  `waybill:closure-member-role` and `waybill:orphan-reason`. The latter loses
  `closure-member-not-an-edge-endpoint` on each moved member, because each now
  has a consumer edge. No non-property field changes.
- **Edges.** Every added and every removed edge touches a moved member, so
  there are 0 unexplained (SC-003).
- **Moat matches the measurement.** Its C184 `edges` (6,891) equals
  `edge-accounting-after.txt`'s `projected, identity level`, and the 16 added
  document edges equal the measured +16. **No moat edge was removed**, which
  resolves the per-edge question `edge-delta.md` left open.
- **Parity.** Unchanged on all five targets: 234 of 239 universal rows pass
  before and after, and the C206 row is identical. The single failing row is
  A5 before and after, so it pre-exists this change.

### Explained differences beyond FR-007's list

1. **C184 `identifiers.wanted` and `licenses.wanted` rise by the moved
   count**, for example moat 1,237 → 1,242. `received` is unchanged.
   - The nixpkgs-metadata lookup counts over `coverage_drv_keys`, which is
     `emitted_drv_keys` plus the evaluated package. A moved member is now an
     endpoint, so it joins the population like every other `source` member,
     and like them it has no `meta` to receive.
   - This is a consequence of FR-004 rather than a separate change.
2. **shellcheck: 5 edges removed.** That document's subject fans out to every
   non-endpoint member: the old golden had 246 `fragment` and 267
   `scaffolding` targets from the subject. Each moved member now has a real
   consumer, so it leaves the fan-out. All 5 removed edges are
   `subject → <moved patch>`.
3. **openai and slack-web gain about 38 edges per patch, against 2–4 on
   moat.**
   - In those two closures `stdenv-linux` is itself a pname-less `fragment`
     (two derivations each), so the walk passes through it to the whole
     toolchain (`gcc`, `glibc`, `binutils`, `gnumake`, …). On moat and
     shellcheck `stdenv-linux` is a versioned package and the walk stops
     there.
   - A patch built with stdenv now has the same fan-out every other `source`
     member built with stdenv already has in those closures. The count
     follows from the stdenv classification, not from this feature.
   - *Observation, out of scope: `stdenv-linux` being labelled `fragment`
     ("a live-bootstrap C function") in those closures is a mislabel of the
     same kind as #1502.*
4. **`waybill:graph-completeness-reason`.** The orphan count in the reason
   string falls by the moved count on moat, openai and slack-web.
