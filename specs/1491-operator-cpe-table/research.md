# Research: Operator-supplied CPEs, and the curated CPE table as data

All findings are dated 2026-11-13 and taken at `ae2549f0`.

## R1 — Every dictionary observation has committed evidence, but two have none at their current count

**Measured.** `measurements/evidence-attribution.py` reads the 95 `NVD_DICTIONARY_OBSERVED` tuples from `cpe.rs`. It indexes every tracked file under `specs/` and `scripts/` in the two shapes the probes have written:
- `vendor:product<ws>N`, from the dictionary probes;
- `product<TAB>vendor<TAB>N`, from the #1490 vendor census.

Output: `measurements/evidence-attribution-2026-11-13.tsv`.

- **93 of 95** observations appear, at exactly the recorded count, in at least one committed file.
- **2 do not:**
  - `openssl:openssl 368`, from the 2026-10-24 run of `scripts/probe-cpe-dictionary.sh`, whose output was never committed. Committed files record 385 (`1375/nvd-pair-probe.tsv`, `1373/nvd-sweep.tsv`) and 387 (`1283/.../nvd-probe-2026-11-11.txt`, `1375/nvd-probe-2026-11-12.txt`).
  - `libssh:libssh 53`. Committed files record 58 (`1375/nvd-vendor-cves-2026-11-13.tsv`).

**Where the 93 matches are.** By file, the exact matches are:

| File | Exact matches |
|---|---|
| `1375/nvd-vendor-cves-2026-11-13.tsv` (the #1490 census) | 93 |
| `1375/nvd-pair-probe.tsv` | 57 |
| `1375/nvd-probe-2026-11-12.txt` | 10 |
| `1283/.../nvd-probe-2026-11-11.txt` | 5 |

The census is the only complete sweep: it holds a row for all 95 pairs, including `openssl:openssl` and `libssh:libssh` at 387 and 58.

**Decision.** Every built-in pair's `dictionary_evidence` is the census, `specs/1375-generic-cpe-table/measurements/nvd-vendor-cves-2026-11-13.tsv`, with `observed = 2026-11-13`.
- The 93 matching counts are unchanged.
- The 2 that differ take the census values: `openssl:openssl` 368 → 387, and `libssh:libssh` 53 → 58.
- The counts are test-only. They are never emitted, so FR-014 (byte-identical goldens) is unaffected.

The earlier probe files stay where they are, as the history of how each row was chosen. The FR-013 evidence check reads the census's dictionary-count column and makes the match permanent.

**How to read the census.** The FR-013 check matches a data row on `product` and `vendor`, then reads `dictionary-entries` by its header name. It skips `#` lines.

The file's `# control+  openssl:openssl  330` line is a CVE count. The probe above picked it up as a second "count" for `openssl:openssl`. Reading by position or by pattern would repeat that mistake.

**Alternatives.**
- *Re-probe NVD now:* rejected. It adds a network step to a refactor whose claim is "nothing changed".
- *Keep 368 and 53 with no evidence:* rejected. A data file that names an evidence path must not point at one that disagrees with it.
- *A per-pair file and date, taken from each observation's `// Probed …` comment:* rejected. Four files and four dates for one table, when one complete sweep exists, is bookkeeping without information. A later re-probe replaces the whole sweep anyway.

## R2 — Two evidence paths per pair, not one

The table is guarded by two kinds of evidence:
- `every_table_pair_exists_in_the_nvd_dictionary` uses the dictionary count;
- `m1375_every_product_has_a_committed_title_query` uses the title query, which says the pair is the same software and not just that it exists.

The title check today concatenates six hard-coded `include_str!` files.

**Decision.** Each pair carries both `dictionary_evidence` and `title_evidence` paths. Both tests read the paths from the data file and open them relative to the repository root at test time. That stays hermetic: no network, only committed files.

This is a superset of the spec's "the evidence file path" (FR-011). The six-file `concat!` goes away.

## R3 — Format: TOML, one `[[row]]` per product

Rows nest: a key, then ordered pairs, each with evidence. #966's TSV suits flat rows only. The operator writes the same schema by hand (FR-004).

**Decision.** TOML, parsed with `toml = "0.8"`, already a direct `waybill-cli` dependency (`cargo.rs`).

- **Built-in file:** `waybill-cli/src/generate/cpe_table.toml`, embedded with `include_str!` and parsed once on first use.
- **Operator file:** the same schema, read from disk.

One `[[row]]` per product keeps diffs to one row. Comments carry the rationale that lives in Rust comments today, including the NVD-dictionary reasoning for multi-vendor rows.

**Alternatives.**
- *TSV with a pair per line:* rejected. The key repeats on every line, and an operator file in that shape is error-prone.
- *JSON:* rejected. It has no comments, and the rationale must travel with the row.

**A built-in file that fails to parse.** It is a compile-time constant, so `the_builtin_table_parses` makes this a test failure, never a runtime one.
- At runtime a parse error logs ERROR, and the table is treated as empty. Emitting no curated CPE is the closed failure (Principle III).
- There is no `unwrap`, so the crate's `clippy::unwrap_used` deny holds.

## R4 — Special cases stay code (FR-012)

| Behaviour | Where today | Decision |
|---|---|---|
| `openjdk` keeps the leading numeric version | `synthesize_cpes` generic arm | Stays code, keyed by slug. It is one row; a version-transform field would be schema for one row. |
| `go` → `golang:go` | `golang` arm, not the table | Unaffected; the table never held it. |
| `boringssl` deliberately absent | `OMITTED` in `mappings_cover_all_curated_libraries` | Stays a test constant. |
| `ninja`, `cmake`, `libxcrypt` rejected | `m1375_the_rejected_names_emit_no_cpe` | Stays a test, now asserting the names are absent from the data file. |

The built-in path keeps matching `pkg:generic` components on `component.name`, exactly as today, so FR-014 holds by construction. A built-in row's key is written `pkg:generic/<slug>`, and its name segment is the slug.

## R5 — Where operator precedence is enforced

The order of CPE passes in `scan_cmd::execute` today:

1. `scan_fs/mod.rs:1348`: `c.cpes = synthesize_cpes(c)` for every filesystem component.
2. `scan_cmd.rs:4364`: `supplement::merge`. Solo entries get `waybill:source-tier = declared`. On a collision, `conflict.rs:355` keeps the scanner's CPE.
3. The closure tier's merge, which replaces a CPE with the nixpkgs-declared one (`scan_cmd.rs:~7524`). It does so only if the existing CPEs are empty, or equal to the synthesized ones and not `source-tier = declared`.
4. `scan_cmd.rs:5129`: `fill_missing_cpes` fills only empty slots.
5. `scan_cmd.rs:5132`: `record_closure_cpe_sources`.

**Decision.**

- **One marker.** An internal annotation, `waybill:operator-cpe` with value `root-cpe` | `supplement` | `cpe-map`, marks a component whose CPEs an operator surface set.
  - It is registered in `root_selector::is_internal_emission_key`, so it is never emitted.
  - Every later pass treats a marked component as final:
    - step 3's `replaceable` reads the marker instead of `source-tier`;
    - step 4 already skips non-empty slots;
    - step 5 skips marked components.
- **Supplement (FR-003)** at step 2: on a collision, valid supplement CPEs replace the scanner's.
  - Validation is `Cpe::parse` with WARN-and-drop, as the solo path's `valid_cpes` does.
  - `ConflictField::Cpe`'s winner becomes the developer.
  - The marker is set, and C212 is set to `{"basis":"purl","overrode":<old primary, if different>}`.
  - A solo entry with CPEs gets the marker and `{"basis":"purl"}`.
  - `waybill:declared-cpe` is no longer written on a CPE conflict. The operator's value is the emitted CPE, and the scanner's is in the conflict record.
- **`--cpe-map` (FR-004, FR-005)** is a new pass after step 4 and before step 5, so it sees nixpkgs-declared, curated and heuristic CPEs and outranks all three.
  - It skips marked components, because the supplement outranks the map.
  - On a match it replaces the list, sets the marker, and sets C212 with basis `cpe-map` and `overrode` against the current primary.
  - A component with no version gets nothing.
- **`--root-cpe` (FR-001, FR-002)** is applied by the emitters through one shared helper, `subject_cpes(main_module, &RootComponentOverride) -> (Vec<Cpe>, Option<overrode>)`. This mirrors `cpe_source`: one function, so the formats agree by construction.
  - **The subject is a scanned main module:** the helper's list replaces that component's CPEs before emission. Mark it, and set C212 with basis `root-cpe`.
  - **The subject is synthetic or an override:** the three synthetic-root builders read the helper. They are `metadata.rs:1307`, `spdx/document.rs:1037` and `spdx/v3_document.rs:1062`, each of which emits no CPE today.

## R6 — Provenance carriers become source-agnostic

`cpe_source(c)` lives in the closure's `identifiers.rs`. The three emitters call it:
- CycloneDX: `cyclonedx/evidence.rs:241`;
- SPDX 2.3: `spdx/packages.rs:454,463`;
- SPDX 3: `spdx/v3_external_ids.rs:78`.

It returns `NixpkgsDeclared` or `Curated`.

**Decision.** Move `CpeSource` and `cpe_source` to `generate/cpe_source.rs`. Re-export them from `identifiers.rs`, so existing paths compile unchanged.

- **New variant:** `Operator(OperatorSurface)`, checked first. It is driven by the marker, and so applies to every component, not only closure members.
- **Fixed strings:** see `contracts/wire.md`.
- **CycloneDX technique:** `other`, as curated uses, with the surface named in `value`.
- **Ranking:** `Curated` stays closure-only (FR-010), so no golden changes.

## R7 — The subject and C212 parity

C212 is a component-scope annotation.

- **SPDX:** a main-module subject is an ordinary package, so its annotations are emitted.
- **CycloneDX:** the main module is promoted to `metadata.component`, and its annotations reach `metadata.component.properties` only through an explicit list (`metadata.rs:~1560–1610`).
- **The parity extractor:** C212's CycloneDX extractor, `cdx_property_values(doc, field, false)`, must then see the subject's value, or `SymmetricEqual` fails.

**Decision.** Add C212 to the CycloneDX subject propagation list, beside `waybill:package-shape`. Prove parity on a main-module-subject fixture with `--root-cpe`.
- If the extractor does not read `metadata.component`, that is the fix point, and the test shows which.
- A synthetic subject carries no C212 (spec FR-009 as amended), so it has no parity question.

## R8 — `--root-cpe` and `--split`

`--split` emits one document per subproject, each with its own subject. One operator CPE cannot name every subject.

**Decision.** `--root-cpe` `conflicts_with = "split"`. Recorded as a spec edge case.

## R9 — Byte-identity evidence for PR A

The local gate excludes the corpus goldens: they are `#[ignore]`d locally. FR-014 is therefore proven by two runs:
- the in-repo golden tests in `pre-pr.sh`;
- a non-regenerating `public-corpus.yml` dispatch at PR A's head, which must report every target identical.

As a control, a deliberately wrong pair in the data file must fail an in-repo golden or a table test. That shows the data file is the one being read.

## R10 — No vulnerability data

The evidence files R1 cites include `nvd-vendor-cves-2026-11-13.tsv`, which also holds CVE counts.
- The data file stores only the path and the dictionary count.
- The FR-013 evidence check reads only the dictionary-count column.
- No CVE number is copied into the data file or asserted by a test.
