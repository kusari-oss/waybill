# TheRock built SDK: why 645 unreachable components carry no marker (FR-001, SC-001)

**Document:** `measure-full` run 44173368036 in `kusari-sandbox/waybill-fixture-therock-sdk`, `branch.cdx.json`.
- **Ref:** `waybill_ref` = `main` at the time of the run (`b503bd6a`, spec 1540 landed).
- **Target:** the TheRock dist tarball `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz`, scanned without `--root-name`, CycloneDX only.

It is a CI artifact, read locally. **Instrument:** `probe_i7_on_document`.

| measure | count |
|---|---|
| the document's `waybill:graph-completeness-reason` | 645 not reachable from root |
| top-level components | 2,105 |
| nested components (`components[].components[]`) | **644** |
| I7: top-level components unreachable from the subject | **1** |
| … explained | 0 |
| … unexplained | 1: a `pkg:generic` library from binary analysis, not under a test-data directory |

**Finding.** The completeness pass and the I7 check disagree by exactly the 644 nested components:
- **the completeness pass** walks the flattened component list, and counts them unreachable;
- **the I7 check** reads only top-level `components[]`, and never sees them.

Containment as reachability (FR-003, research R2) closes the gap: the 644 sit inside reachable components, and become reachable in both. The one top-level binary takes `unowned-binary` (FR-004 in any scan). The SDK has no other population.

**For FR-009:** `measure-full` emits CycloneDX only. Its scan step gains SPDX 2.3 and SPDX 3 in T013.
