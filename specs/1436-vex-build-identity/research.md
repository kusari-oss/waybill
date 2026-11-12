# Research: Nix closure VEX that scanners honour

Code citations are from main@`ef47f876`. New measurements, taken 2026-11-12, are in `measurements/`:
- `fixed-qualified-probe.sh` and `fixed-qualified-results.tsv`;
- `osv-qualifier-probe.sh` and `osv-qualifier-results.tsv`.

Earlier ones (2026-11-10) are described in `measurements/README.md`.

**Tools:**
- grype 0.120.0 from nixpkgs `0bb2eedc…`: store path `7r24kzxg…-grype-0.120.0`, the same one the 2026-11-10 probes used.
- The grype database is schema v6.1.10, built 2026-11-08 (sha256 `dcca2472…`, `recall-truth.yml`'s pin), imported into an isolated cache.
- osv-scanner 2.7.0.
- Inputs are the five committed closure goldens on main.

## R1 — The qualifier: key and value (FR-009)

**PURL specification**, read at purl-spec `26671718`:
- `docs/specification/common-qualifiers.md` lists `checksum`, `download_url`, `file_name`, `repository_url`, `vcs_url` and `vers`.
- `types/generic-definition.json` defines `download_url` and `checksum` for `pkg:generic`.
- The spec asks for "a minimal set of key=value pairs that are necessary for accurate package identification".
- No `nix` type or Nix qualifier exists in the repository (0 paths match `nix`).

**`checksum` is the established key, and it is rejected on evidence:**
- Its value must be a checksum of the package artifact.
- A Nix store path's hash is not one: it hashes the derivation's inputs.
- The artifact checksum Nix does have is the output's NAR hash (`narHash`). The closure tier never obtains it: waybill reads `narHash` only for `flake.lock` inputs (`nix/lockfile.rs:86`).
- Getting it would cost a `nix path-info` query per patched output, which is new network reach (FR-012). And it would be unavailable for a path not in the local store or a substituter.
- A `checksum` qualifier that could not always be computed would leave some backports unscopable.

**Decision:** a dedicated key, **`nix_drv`**. Its value is the 32-character Nix store hash of the **derivation** (`.drv`) whose patches resolve the CVE: `pkg:generic/unzip@6.0?nix_drv=<hash>`.
- **Deterministic.** The same derivation always gives the same value, so FR-001 and US3-1 hold.
- **Always present.** It is the key of the closure's own derivation map, so no query is needed.
- **Covers every output.** It names the build, not one output of it.
- **The key follows the spec's syntax** (lowercase, `_`-separated like `download_url`). It says what it identifies, and it can't be mistaken for a checksum.
- **It is limited to identification,** which the spec's restraint guidance asks for, and only patched components carry it (clarification 2).

**Alternatives rejected:**
- `checksum` with a NAR hash (above).
- `checksum` with the store hash: a false claim about what the value is.
- An output-path hash: names one output where the patch covers the build.
- A `vcs_url` / `download_url`-style value: says where something comes from, not which build it is.

## R2 — grype honours `fixed` on a qualified product (FR-002's open check)

`fixed-qualified-results.tsv`, over **every** `pedigree.patches[].resolves` pair in each golden. The probe used the placeholder key `build=…`; R3 shows the key doesn't matter to matching.

| target | patched findings | `fixed_q` suppressed | `na_q` suppressed | leak (`fixed` VEX, unqualified doc) | others lost |
|---|---|---|---|---|---|
| nix-closure-moat | 1 | 1 | 1 | 0 | 0 |
| nix-closure-openai | 6 | 6 | 6 | 0 | 0 |
| nix-closure-shellcheck | 1 | 1 | 1 | 0 | 0 |
| nix-closure-slack-web | 4 | 4 | 4 | 0 | 0 |
| nix-closure-slacklinker | 5 | 5 | 5 | 0 | 0 |
| **total** | **17** | **17** | **17** | **0** | **0** |

**Decision:** `fixed`, as clarified. The fallback is not needed. Qualifying the patched components alone (`q_none`) changed no finding count on any target. That is FR-007 for grype.

## R3 — Qualifiers do not cost osv-scanner findings (FR-007)

`osv-qualifier-results.tsv`. Every component PURL in the document is qualified, which is harsher than FR-001.

| document | PURLs qualified | ecosystems with findings | findings before | after | lost | gained |
|---|---|---|---|---|---|---|
| nix-closure-slacklinker | 2,942 | crates.io, Hackage | 55 | 55 | 0 | 0 |
| nix-closure-openai | 1,375 | Hackage | 12 | 12 | 0 | 0 |

**Which components FR-001 qualifies on the goldens:**
- every one is `pkg:generic`;
- moat: libssh2, net-tools, unzip;
- openai: db 4.8.30 and 5.3.28, patch, unzip;
- shellcheck: as moat;
- slack-web: binutils, net-tools, perl, unzip;
- slacklinker: binutils, db, libxml2, lz4, net-tools, unzip.

**Decision:** apply FR-001 to every PURL type alike. A registry PURL with a resolved CVE would take the qualifier with no loss, as measured on crates.io and Hackage. PyPI isn't measured, because no golden has a PyPI finding. The implementation must re-check this on the six wrapper targets (httpie carries `pkg:pypi`).

## R4 — One component, several patched derivations

A closure component is one identity `(pname, version)`, and it may merge several derivations (build variants). This is **common for exactly the components this feature is about**. `patches.rs` `merge_by_identity` records: "`unzip 6.0` appears twice in a real closure as two build variants, each applying the same 17 patches". `unzip` is the source of most patched findings, 11 of 17 per target once #1435's CPE lands. The patch sets are unioned per identity, and the derivations that carried them are not kept (R11).

**First decision, superseded at tasks time:** leave a multi-derivation component unscoped, with no statement. That would have given `unzip` no claim and made SC-001 unreachable, so it was revisited, as this section had said it would be if the case proved common.

**Decision:** the qualifier names the **set** of derivations the component stands for, and a claim is made only where it holds for all of them (Principle IX).
- **The value:** `nix_drv` is the sorted, comma-joined store hashes of every derivation merged into the component that applies patches. With one such derivation it is a single hash.
  - The order is fixed, so the value is deterministic.
  - A scanner treats it as opaque: product PURL and package PURL compare equal as strings.
- **The claim:** a `fixed` statement for a CVE is emitted only when **every** one of those derivations applies a patch that resolves it. Then the claim is true of each build the component describes.
- **The count:** a CVE that only some variants resolve gets no statement, and it is counted in C220 `waybill:vex-claims-unscoped`.
- **Accuracy:** a statement can never cover a variant that lacks the patch. A different set of variants (another closure, another revision) has a different qualifier, so the leak probe's guarantee holds.
- **Rejected: splitting the component into one per variant.** It changes component counts and edges across the document for a claim that, as measured, is identical across the variants.
- **Unmeasured, to measure at implementation:** how often variants disagree on a CVE, recorded per target.
- **Measured at implementation (T001, 2026-11-12):** never, on the five nixpkgs wrapper targets. Of 4–6 components per target with a resolved CVE, 2–3 merge two patching derivations, and 0 of 18–23 (component, CVE) pairs per target are resolved by only some of them. No CVE-resolving component merges a derivation that applies no patch either. See `measurements/variants-results.tsv`.

## R5 — CycloneDX state for a backport claim (FR-005)

`cyclonedx/vex.rs:22` `cdx_state` maps `Fixed → "resolved"` for every statement.

**Decision:** `resolved`, the existing mapping, so no per-origin mapping is needed.
- The plan first chose `resolved_with_pedigree`. The vendored schema (`tests/fixtures/schemas/cyclonedx-1.6.json`) defines it as "remediated and evidence of the changes are provided in the affected components pedigree containing verifiable commit history and/or diff(s)".
- On `nix-closure-moat`, waybill's patch entries are 15 × `{resolves, type}` and 186 × `{type}`, with no `diff` or `commits`. The stronger state would claim evidence the document doesn't hold, so the user chose plain `resolved` ("The vulnerability has been remediated.").
- Earning `resolved_with_pedigree` would mean adding each patch's diff to `pedigree.patches[].diff`. That is out of scope, and a possible follow-up.
- `affects[].ref` is the component's bom-ref (FR-005), and the `waybill:vex-subcomponent` property is no longer used for backports. Catalog row F1 keeps "`resolved_with_pedigree` is never emitted".

## R6 — Where the qualifier is applied, and the edges (#980)

**A PURL is identity:** `bom-ref`, `Relationship::from/to`, SPDX ids and the OpenVEX product all follow it. Changing one after edges exist silently orphans them (#980; the CycloneDX edge stays, SPDX drops it).

**Decision:**
- **Where:** the qualifier is applied in the closure tier when patched components are emitted (`closure/emit.rs`, where `ANN_CLOSURE_PATCHES` and the derivation keys are both in hand).
- **When:** before the closure graph's edges are projected, so the edges are built from the qualified PURL.
- **Merges:** any rename after that point (a merge into a manifest-derived component of the same PURL) goes through `apply_renames`.
- **The rule for merges:** a closure member whose unqualified PURL equals a manifest component's keeps the merge. The qualified PURL wins, because it is the more specific identity of the same component.
- **Tests:** an orphan check (every relationship endpoint is a component) runs on every fixture.

**Implementation note (2026-11-12).** `emit::components` applies the qualifier, as decided. It does not survive there on its own: three passes between emission and the edge build — the milestone-1275 Hackage identity pass (`closure_identity.rs`), the milestone-1366 ecosystem identity pass and the milestone-1378 contents pass — rebuild a component's PURL from its parts, and a member merged into another reader's component takes that component's PURL. So the scan applies it again (`emit::qualify_backport_identities`, idempotent) immediately before `closure::graph::relationships`, and moves the endpoints of edges that already exist with `apply_renames`. The closure edges are then built from the qualified PURL. A merged component keeps its own PURL type and gains the qualifier (test `m1436_a_patched_member_merges_and_the_merged_component_is_qualified`).

The every-variant CVE set reaches the VEX generator in memory, as T006 asked, through an internal annotation (`waybill:nix-backport-identity`) filtered from every format by `is_internal_emission_key` — the mechanism `waybill:nix-evaluated-package` already uses — so no new emitted field and no new plumbing. A statement is emitted only when the component's PURL actually carries the recorded `nix_drv` value, so a PURL that could not be qualified never receives a build-scoped claim.

## R7 — The statements (FR-002–FR-004)

`openvex/mod.rs:292` `backport_statements` today:
- groups by CVE;
- emits `affected` on the component PURLs;
- emits `not_affected` with the root as product and the components as subcomponents;
- returns nothing without a root.

**Decision:**
- One `fixed` statement per (qualified component PURL, CVE): product `@id` is the qualified PURL, there are no subcomponents, and `status_notes` / `impact_statement` carry the m1035 grade note and provenance (FR-004).
- **No root is needed.** The product is the component, so a document without a subject still gets its statements. That removes #1436 part 1's failure mode by construction.
- The `affected` half is dropped (FR-003). The version-level claim stays in `pedigree.patches[].resolves`.
- One statement per pair, not one per CVE across components. Each product is a distinct build, and a per-pair statement keeps each claim's evidence grade separate.

## R8 — Measuring SC-001 and SC-002

- **Committed goldens (CI-reproducible).** `fixed-qualified-probe.sh` already measures the target shape: 17 of 17 suppressed, 0 leaked, 0 lost. After implementation, the same probe runs on the **regenerated** goldens with the **emitted** sidecar in place of the synthetic one. That is the gate.
- **The six spec targets** (curl, openssh, httpie, gh, ripgrep, moat): wrapper flakes over nixpkgs `a799d3e3` are rebuilt locally at implementation time. Only nixpkgs is evaluated, and no third-party flake is built. Re-measure the baseline, then run the same probe. The 2026-11-10 figure (102 with #1435's CPE, 36 without) is not reused.

## R9 — What does not change (FR-008)

- m1050's nixpkgs security declarations keep their statements and shape.
- Non-closure scans emit no backport statements, so they are byte-identical (SC-005).
- Components without a resolved CVE keep their PURL (FR-001, clarification 2).

## R10 — A nixpkgs declaration still withholds the claim (FR-008)

`openvex/mod.rs:163` `is_withheld_not_affected` (m1050, FR-012 there):
- when `meta.knownVulnerabilities` declares a version vulnerable, the patch-derived `not_affected` is withheld;
- the match is on (subcomponent, CVE);
- test: `a_declaration_withholds_the_patch_derived_not_affected`.

nixpkgs then says, in its own words, that this build's version is vulnerable. A `fixed` claim must not override that.

**Decision:** the rule carries over to the new shape.
- A backport `fixed` statement is withheld when a declaration names the same (component, CVE).
- The match is on the product PURL with qualifiers removed, plus the CVE.
- The declaration's own statements are unchanged (FR-008).
- **Implementation note (2026-11-12).** The withholding is keyed on a declaration *existing*, not on its statement being emitted. Since a backport claim no longer needs a document subject, it is emitted on a scan with no subject, where the declaration (which still names the build) is not. Withholding only beside an emitted declaration would let the claim stand against nixpkgs' word. `declarations::withheld_pairs` also compares PURLs without qualifiers now, so the count it feeds (`reconciliations-withheld`) matches the behaviour.

## R11 — Where patch records come from

`closure/emit.rs:636` attaches `ANN_CLOSURE_PATCHES` from an index keyed by `(name, version)`. The derivation that carried each patch is not kept.

R4 needs it, so before merging, each derivation's patch set must be kept beside the union. A component then knows its full set of patching derivations, which become the qualifier value, and which CVEs every one of them resolves (R4).
