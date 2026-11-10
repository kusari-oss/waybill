# Contract: documentation

| file | change |
| --- | --- |
| `docs/reference/sbom-format-mapping.md` | C182: add the `root` value. New rows C213, C214, C215 in Section C, format per the C210 row. |
| `docs/ecosystems.md` | Closure tier section: what is read, that nothing is built, what `absent` means, the strict flag, `NIX_STORE_DIR`. |
| `waybill-cli/src/cli/scan_cmd.rs` | `--nix-closure` help text gains one paragraph; new flag's help text. |
| `CHANGELOG.md` | Entry under Unreleased, if the file tracks unreleased changes. |

Gates that check the catalog rows: `tests/holistic_parity.rs`,
`tests/sbom_format_mapping_coverage.rs`, `tests/mapping_doc_bidirectional.rs`.
The doc row and its extractors land in one commit (the gate is in the lib
target).
