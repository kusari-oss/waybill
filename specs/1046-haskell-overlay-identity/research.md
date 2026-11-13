# Research: 1046 — one identity for a Nix-built Haskell dependency the package set does not carry

All measurements were taken on main `2aab3baf`, 2026-11-13, unless stated otherwise.

## R1. Where the twins come from, and where the merge belongs

**Finding.**
- **Why the twins never merge today.** `merge_closure_components` (`cli/scan_cmd.rs`) folds a closure member into an existing component only on an exact `(name, version)` match. A versionless design-tier component never matches.
- **What the identity pass does next.** `closure_identity::reclassify` (m1275) then gives remaining `pkg:generic` closure members a Hackage identity when the pinned set holds the exact pair *and* a derivation was built by a GHC (`decide`). Otherwise it records C201 `waybill:closure-identity` (`version-disagrees`, `not-in-package-set`, …).
- **The two twins on the corpus:**
  - `slack-web` (slacklinker): `version-disagrees`, so it stays `pkg:generic`;
  - `hspec-discover` (slack-web): `confirmed`, so it is already `pkg:hackage`, beside a versionless design-tier twin.

**Decision.** Add a twin-merge pass that runs immediately after `reclassify` (and after R2's publication step), and before `qualify_backport_identities` and `graph::relationships`.
- At that point every build-tier identity is final.
- Edges that already name the design-tier PURL (the manifest's) are moved with `apply_renames`, by the #980 convention.
- The closure's own edges are built afterwards from final PURLs, through `member_key` (#1473).

**Alternatives.**
- **Extending `merge_closure_components` to match a versionless design-tier component.** Rejected: it runs before the identity pass, so `slack-web` would merge as `pkg:generic` and lose the chance at R2's identity, and m1035 measured that reordering the merge downgrades PURLs.
- **Reusing the m191 reconciler.** Rejected: it matches within one ecosystem (`(ecosystem, canonical_name, manifest_dir)`), and a merge that must still happen when the Hackage check cannot run (offline) pairs `pkg:hackage` with `pkg:generic`.

## R2. Which closure members are candidates for a Hackage-published identity

**Finding.** C201 counts per closure golden:

| target | `version-disagrees` | `confirmed` |
|---|---|---|
| moat | 31 | 106 |
| openai | 27 | 91 |
| shellcheck | 32 | 333 |
| slack-web | 27 | 108 |
| slacklinker | 37 | 123 |

`version-disagrees` is mostly the collision population m1275 documented (GNU `bash 5.3p9` against Hackage's `bash`). On slacklinker, only two of the 37 look like Haskell packages: `slack-web@2.3.0.0` and `tmp-postgres@1.35.0.0`, and both are overlays (`nix/deps/*.nix` at the pinned commit).

**Decision.** A member is a candidate for the Hackage lookup only when it is `version-disagrees` **and** `built_by_ghc` holds for one of its derivations. `version-disagrees` already means the pinned set carries the name, so only public names reach Hackage. A `not-in-package-set` member, which may be a private package such as the project itself, is never sent (spec FR-006a). That is the second signal m1275 already requires, which rejected 26 of 26 measured collisions. Everything else makes no request. So a scan makes about as many requests as it has Haskell overlays (inference from this one target; 2 on slacklinker).

**Alternatives.** Looking up every `version-disagrees` member was rejected: it would send `bash`, `curl` and `zlib` to Hackage and invite exactly the collision m1275 exists to prevent.

## R3. How to ask Hackage "is `<name>` `<version>` published?" (measured)

`measurements/hackage-probe.sh` gives the probe, and `measurements/hackage-probe-2026-11-13.txt` its output.

| query | answer |
|---|---|
| `GET /package/<name>-<version>/<name>.cabal`, published | 200, the `.cabal` (`slack-web-2.3.0.0`, `HUnit-1.6.2.0`) |
| same, version not published | 404, "No such package version for `<name>`" (`slack-web-2.3.0.1`, **`tmp-postgres-1.35.0.0`**) |
| same, wrong case | 404 (`hunit-1.6.2.0`): names are case-sensitive |
| `GET /package/<name>/preferred` (JSON) | 200, `{"normal-version":[…],"deprecated-version":[…]}`; 404 for an unknown name |
| latency | 213–420 ms on the per-version endpoint (one 1.3 s outlier in the first session) |
| burst | 20 sequential per-version requests, all 200, no throttling seen |

**Decision.** Use the per-version endpoint, one GET per candidate.
- **200 means published, 404 means not published, and anything else (timeout, 5xx, transport) means unreachable.**
- **Name as recorded.** The closure's `pname` is used as recorded, never case-folded.
- **Bounded.** A per-request timeout, and the scan's existing network budget conventions (`HttpSource`'s dedicated-thread pattern, because `reqwest::blocking` inside the async `execute` panics on drop).
- **Cached for one scan only.** No disk cache in this milestone: about two requests per scan does not justify one.

**Alternatives.** The per-name `preferred` list was rejected. It costs one request either way, but it carries data the decision does not need, and its `deprecated-version` split adds a case to decide. A deprecated version is still published, but the probe found no example to measure, so the per-version endpoint's plain 200/404 is the simpler fact.

**Determinism caveat.** A golden that depends on Hackage can change when a version is later published. `tmp-postgres 1.35.0.0` would move from "not published" to "published" if upstream released it. This is accepted: the document then says something truer, and the change is visible as a C201 value change in the regenerated golden.

## R4. The `vcs_url` qualifier

**Decision.** For a candidate Hackage publishes, built from a VCS checkout (R5), the identity is `pkg:hackage/<name>@<version>?vcs_url=git+<url>@<rev>`. The purl-spec defines `vcs_url` as a VCS URL in SPDX's `<vcs_tool>+<transport>://<host>/<path>@<revision>` form.
- **Encoding.** `Purl::new` canonicalises qualifier order but does not percent-encode qualifier values (`waybill-common/src/types/purl.rs`). The value's encoding is checked by a unit test against the purl-spec rules, not assumed.
- **Built from the Hackage tarball.** No qualifier: `pkg:hackage/<name>@<version>`.
- **Not published, or the check did not run.** `pkg:generic` is unchanged (FR-006a, FR-007).

**Alternatives.**
- **A `download_url` qualifier.** Rejected: it names an artifact URL, not a VCS revision.
- **Recording the VCS source only as an external reference, with no qualifier.** Rejected by the clarification (Q1 → B): the qualifier keeps the build's provenance inside the identity a matcher reads.

## R5. Where a git fetch records its URL and revision (measured)

**Finding.** At nixpkgs `a799d3e3`, `nix derivation show` of a `fetchgit` derivation has `env` = `{out}` only. `url` and `rev` live under `structuredAttrs`.
- **The emitter.** `emit.rs::source_url` reads `drv.env` only. `RawDerivation::attr_str` already reads both carriers (m1278), but `source_url` bypasses it.
- **The result on slacklinker.** 111 of 902 `source` members carry a reference, and `slack-web-1ea1e48` (the git checkout) carries none.

**Decision.**
1. `source_url` reads through `attr_str`. A structured `urls` may be a list; take its first string.
2. A source member that records both `url` and `rev` gets a `vcs` external reference `git+<url>@<rev>`, beside (not instead of) any `distribution` reference.
3. A package component gets the same `vcs` reference when its derivation's `src` is the output of such a member, joined by output path.

**The `vcs` reference needs no new catalog row.** A10 already maps it natively in all three formats:
- CycloneDX `externalReferences[type=vcs]`;
- SPDX 2.3 `externalRefs` `OTHER`/`vcs`;
- SPDX 3 `software_sourceInfo`.

**Alternatives.** Inferring a VCS checkout from a member's name (`slack-web-1ea1e48` embeds a short revision) was rejected as name-shape inference, the defect #1374 records.

## R6. What the merged component carries

**Decision.** The survivor is the build-tier component: its PURL, version, tier, scope and closure annotations.
- **From the design tier it gains** C20 `waybill:requirement-ranges`, the source-file evidence, and any annotation it lacks. That is the fill-only-absent rule `merge_closure_components` uses for licenses.
- **Dropped, because they become false:** the design tier's "unresolved" annotations (`waybill:haskell-version-unresolved-reason`, C151 `waybill:unresolved-reason`).
- **Kept, because it stays true:** C172 `waybill:nixpkgs-version-disagreement`, which says the pinned set carries another version.
- **Edges** naming the design-tier PURL are renamed to the survivor's, and duplicates are removed.

**Tier.** The survivor keeps `build`. This follows the m191 precedent, where the more concrete tier survives. Consequence: a `--tier design-only` scan no longer lists a dependency whose twin was merged. That is the same behaviour m191 gives design/source twins today.

## R7. Recording a build outside the declared range

**Decision.** A new per-component row, **C221 `waybill:requirement-unsatisfied`**, emitted only on a merged component whose build version every declared range rejects (`version_range::contradicts`). Its value is `{"version": "<built>", "requirement-ranges": [...]}`.
- **Principle V audit.** None of CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 has a field relating a declared version constraint to the version resolved. CycloneDX `component.scope` and `pedigree` describe different things, and SPDX has no constraint slot. So: KEEP-NO-NATIVE, shaped on C172's per-component JSON object.
- **When a range does not parse** (`Admits::Unknown`), the build is not recorded as unsatisfying: no claim without evidence.

## R8. The Hackage check's outcomes, recorded

**Decision.** For a GHC-built `version-disagrees` member, C201 `waybill:closure-identity` gains five values:
- `hackage-published` — the identity was minted from Hackage;
- `hackage-unpublished`;
- `hackage-unchecked-offline`;
- `hackage-unchecked-opted-out`;
- `hackage-unreachable`.

`version-disagrees` keeps its meaning for every other member, the collisions.
- **The summary.** The pass's summary (`ClosureIdentitySummary::to_document_value`) is logged, not emitted as a document row, and counts the new values with no code change to its shape.
- **Provenance.** `hackage-published` is the provenance Principle XII.2 requires for an identity from an external source.

**Alternatives.** A separate new row for the check was rejected. C201 already exists to answer "why does this closure member have, or lack, a Hackage identity", which is this question.

## R9. The opt-out flag

**Decision.** `--no-hackage-lookup`, following `--no-deps-dev` and `--no-nixpkgs-haskell`: it disables this lookup only. `--offline` also refuses it. Neither flag changes the merge (FR-001); both leave `pkg:generic` with the C201 reason from R8.

## R11. A `vcs_url` qualifier does not stop advisory matching (measured)

`measurements/qualifier-match-probe.sh`, output in `measurements/qualifier-match-probe-2026-11-13.txt`.
- **Positive control.** HSEC-2024-0003 affects `process` from 1.0.0.0, fixed in 1.6.23.0. `pkg:hackage/process@1.6.18.0` returns it from the OSV API with no qualifier, with a percent-encoded `vcs_url`, and with an unencoded one. osv-scanner 2.7.0 on a CycloneDX document returns it with and without the qualifier.
- **Negative control.** `@1.6.23.0` returns nothing, with or without the qualifier, so the match is version-sensitive and not name-only.

**Decision.** Option B's premise holds for the Hackage consumers measured: the qualifier records the provenance and costs no match.

## R10. Expected golden changes

**Inference from R1–R5, to be measured by the regeneration:**
- **slacklinker:**
  - `slack-web` merges and becomes `pkg:hackage/slack-web@2.3.0.0?vcs_url=…`, carrying a `vcs` reference;
  - `tmp-postgres@1.35.0.0` records `hackage-unpublished`;
  - its git `source` members gain `vcs` references.
- **slack-web:** `hspec-discover` merges and carries C221.
- **Every closure target:** structured-attribute fetches gain `distribution` and `vcs` references. The count above 111 on slacklinker is SC-003's measurement.
- **Untouched:** non-closure targets change byte-for-byte nothing.
