# Quickstart: verifying spec 1491

All commands run from the repository root. Fixtures use synthetic `waybill-fixture-*` names.

## PR A — the table move (US4)

1. **Gate.** Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Expect `GATE_EXIT=0`, with every table test passing against `cpe_table.toml`.
2. **Byte-identity.** Dispatch `gh workflow run public-corpus.yml --ref 1491-operator-cpe-table -f branch=1491-operator-cpe-table -f regen_goldens=false`. Expect every target identical (158 of 158 at the last run).
3. **Control.** Swap the vendor in one row of `cpe_table.toml` and rerun `cargo test -p waybill --lib generate::cpe`. A table test, or the in-repo golden for that row's product, must fail. Revert with `cp`.
4. **Evidence check.** Point one pair's `dictionary_evidence` at a path that does not exist. The FR-013 evidence test must fail and name the row. Revert with `cp`.
5. **No vulnerability data.** Check that `cpe_table.toml` has no `cve`, `CVE-`, `advisor` or `vuln` field or value:

   ```
   grep -nEi 'cve|advisor|vuln' waybill-cli/src/generate/cpe_table.toml
   ```

   Expect no output, apart from comment text explaining why there are no such fields, if any.

## PR B — the operator surfaces (US1–US3)

1. **Subject.** Run:

   ```
   waybill sbom scan --path <fixture> --root-name waybill-fixture-app --root-version 2.1.0 \
     --root-cpe 'cpe:2.3:a:waybill-fixture-vendor:waybill_fixture_app:2.1.0:*:*:*:*:*:*:*' \
     --format cyclonedx-json,spdx-2.3-json,spdx-3-json
   ```

   In all three formats, the subject carries the CPE and the comment or method `CPE supplied by the operator with --root-cpe`.
2. **Malformed root CPE.** Pass `--root-cpe 'cpe:2.3:a:*:x:1:*:*:*:*:*:*:*'`. Expect a non-zero exit and no output file.
3. **Supplement over a guess.** Supplement a fixture `deb` component with a valid CPE. The emitted primary CPE is the operator's, C212 is `{"basis":"purl","overrode":"cpe:2.3:a:debian:..."}`, and the conflict record says developer-wins.
4. **Map.** Use a `--cpe-map` file with one row keyed `pkg:deb/debian/waybill-fixture-lib`, on a fixture holding that package at two versions. Both versions carry the mapped vendor and product at their own version, with basis `cpe-map`.
5. **Precedence.** Give one component both a map row and a supplement CPE. The supplement's CPE wins.
6. **Parity.** The C212 parity row reports equal sets on every fixture above, including the main-module subject case (research R7).
7. **Default output.** With no new flags and no supplement, the corpus check stays identical.
