# FR-001 trace: why one SDK component answers differently from scan to scan

**Instruments:**
- `measurements/probe_repro.rs`, a throwaway synthetic reproduction. It was run as an ignored test, from `waybill-cli/tests/`, at `main` `37de8e08`.
- The SDK documents from `measure-full` runs 44189121691 and 44193745935 (CI artifacts, read locally).

TheRock was not scanned locally.

## What the SDK documents show

The top-level `pkg:generic/gotcha` (evidence kind `embedded-source-path`, spec 1534's bundled tree) has one occurrence, `lib/rocprofiler-sdk/libgotcha.so`, with hash **`56af308b…`**. That is the content of `libgotcha.so.3.0.0`. Yet the same path is listed under `pkg:generic/libgotcha.so?file-sha256=10e333b8…`, whose content is rocprofiler-systems' gotcha 2.4.2.

The two `libgotcha.so` are ends of two symlink chains in different directories: `.so` → `.so.3` → `.so.3.0.0` and `.so` → `.so.2` → `.so.2.4.2`. **They share a name, not a content hash.** The spec's input said otherwise, and the spec is corrected.

## Reproduction

Two symlink chains, `lib/sdk/libwaybill-fixture-g.so` → `.so.3` → `.so.3.0.0` and `lib/systems/libwaybill-fixture-g.so` → `.so.2` → `.so.2.4.2`, ending in different bytes. Each embeds one bundled-tree path (`third_party/waybill-fixture-gotcha/` and `third_party/waybill-fixture-timemory/`). Scanned 12 times:

| | result |
|---|---|
| file components named `libwaybill-fixture-g.so` | **one** (`?file-sha256=9438…`, the sdk chain's), in all 12 |
| `timemory` from the systems `.so` | **top-level**, its declared parent (`libwaybill-fixture-g.so?file-sha256=dd03…`) not emitted, in all 12 |
| completeness orphan count | **1 in 4 scans, 0 in 8**; the document is identical in all 12 |

This mirrors the SDK, with the roles of the two directories swapped.

## Cause 1: the deduplicator merges different files by name

`resolve::deduplicator::deduplicate` groups by `(ecosystem, name, version, parent_purl, pants namespace)`, deduplicator.rs:13. Two file-level components with one file name and different `file-sha256` qualifiers fall into one group. The survivor keeps its own PURL, absorbs the other's source paths, and drops the other's PURL.

Every identification read from the losing file still names that PURL as `parent_purl`. The CycloneDX builder finds no such component and demotes the identification to the top level (builder.rs, "orphans are demoted to top-level"). SPDX 2.3, keyed by PURL, folds it into the same-PURL package nested elsewhere.

**Single-variable test:** adding the `file-sha256` qualifier to the group key, alone, makes both file components emitted, and every identification nested under its own file, in 12 of 12 scans.

## Cause 2: spec 1541's containment edges depend on component order

`graph_completeness::add_containment_edges` (spec 1541) runs three rules in one loop over the components:

| rule | what it does |
|---|---|
| R1 | edge from container to child |
| R2 | copy the child's PURL's outgoing edges onto the child's key |
| R3 | for a PURL with no top-level component, edge from the bare PURL to each nested copy |

R2 reads `edges[purl]` while R3 writes to it in the same loop. When a nested copy is visited after another copy of the same PURL, R2 copies R3's links, and the copy gains edges to its same-PURL siblings, the detached one included. That makes the detached identification reachable.

The deduplicator returns components in `HashMap` order, which varies between processes. So reachability, and with it the orphan count, the classifier's codes and I-A, varies while the document does not.

**Single-variable test:** taking R2's edges from a snapshot of the dependency edges, taken before any containment edge is added, alone, makes the orphan count identical in 12 of 12 scans (1, the detached `timemory`, coded `unowned-binary`).

**Both changes together:** 12 of 12 scans emit both file components, every identification nested, no orphan, and identical documents.

## Not yet measured

- Whether anything else varies between scans: on the SDK, and on every public corpus target (FR-006, T-audit).
- The deduplicator's survivor and output order also follow input order, for groups whose members differ only in fields outside the key (for example a `deb ?arch=` variant). Whether that varies in practice is part of the same audit.
