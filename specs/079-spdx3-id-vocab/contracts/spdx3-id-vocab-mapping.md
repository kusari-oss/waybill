# Contract — milestone 079 SPDX 3 externalIdentifierType conformance

The milestone's only contract.

## CLI surface

**No new flags.** The conformance pass is a wire-format change to existing SPDX 3 emission. Operators see no new flags on `mikebom sbom scan` or `mikebom trace run`.

## Library surface (`mikebom-cli` crate)

**No new public Rust API surface.** The new `v3_id_type_map.rs` module's items are pub(crate) — only the SPDX 3 emission code path consumes them. CDX 1.6 + SPDX 2.3 emission paths do not.

Internal pub(crate) items:
- `SpdxIdType` enum (11 variants matching the SPDX 3 controlled vocabulary)
- `MappingResult { vocab_type, comment }` struct
- `map_scheme_to_vocab(scheme: &SchemeName, value: &str) -> MappingResult` pure function
- `is_git_sha(value: &str) -> bool` helper

## Validator integration surface

**No new validator integration.** Reuses milestone 078's `scripts/install-spdx3-validate.sh` (pinned at `spdx3-validate==0.0.5`), `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` env-var hook, and `mikebom-cli/tests/spdx3_conformance.rs` integration test infrastructure. The milestone extends the test file with new test cases (per the plan); no new shell script, no CI workflow update.

The existing milestone-078 `every_existing_golden_passes_validator` test automatically gains coverage for any new fixtures that exercise the milestone-079 mapping path. The existing 9 source-tier ecosystem fixtures don't exercise it (manifest-only) and stay byte-identical.

## Wire-format contract (per SPDX 3 model)

### `Core/ExternalIdentifier` element shape

```json
{
  "type": "ExternalIdentifier",

  // SHACL: required, value MUST ∈ {other, cve, swhid, securityOther,
  //        cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid}
  "externalIdentifierType": "<one of the 11 vocab values>",

  // SHACL: required, value is the identifier string itself
  "identifier": "<value as supplied by mikebom's identifier layer>",

  // SHACL: optional, type=string. Emitted ONLY when the mapping
  //        loses information (i.e., the original mikebom scheme
  //        name doesn't equal the emitted vocab value). Format:
  //        `"original-scheme: <scheme>"`.
  "comment": "original-scheme: <mikebom-scheme-name>"
}
```

### Per-scheme mapping (definitive — see research §1)

| Input scheme | Input value shape | Output `externalIdentifierType` | Output `comment` |
|---|---|---|---|
| `image` | any | `other` | `"original-scheme: image"` |
| `repo` | any | `other` | `"original-scheme: repo"` |
| `git` | matches `^[0-9a-f]{40}$` (SHA-1) | `gitoid` | (omitted) |
| `git` | does NOT match the regex | `other` | `"original-scheme: git"` |
| `subject` | any | `other` | `"original-scheme: subject"` |
| `attestation` | any | `other` | `"original-scheme: attestation"` |
| `<vocab>` (e.g., `cve`, `cpe23`, `gitoid`, etc.) | any | the vocab value verbatim | (omitted) |
| `<non-vocab user-defined>` (e.g., `jira`, `internal-ticket`) | any | `other` | `"original-scheme: <name>"` |

### Determinism contract

- Same `(scheme, value)` input → byte-identical `MappingResult` output across re-runs.
- The `gitoid` regex is compiled once via `OnceLock<Regex>`; no per-call recompilation.
- The `externalIdentifier[]` array sort key extends from `(type, identifier)` to `(type, identifier, comment)` so the multi-source dedup case stays deterministic + correct (per research §4).

## Observable contract

### Pre-fix: validator output (per the spec.md reproduction recipe)

```bash
$ MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --test spdx3_conformance fresh_image_tier_emission_passes
# (with non-empty RepoTags)
ERROR: SHACL Validation failed for ...:
Violation of type sh:ClassConstraintComponent:
  Source Shape: sh:in (other cve swhid securityOther cpe23 packageUrl gitoid cpe22 urlScheme email swid) ;
                sh:path Core/externalIdentifierType
  Value Node: "image"
  Message: Value is not in {other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid}
$ echo $?
1
```

### Post-fix: validator output

```bash
$ MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --test spdx3_conformance
# (all tests including image / source-with-git / build-tier / user-defined)
test image_tier_with_repo_tags_passes_validator    ... ok
test source_tier_in_git_repo_passes_validator       ... ok
test build_tier_with_subjects_passes_validator      ... ok
test user_defined_scheme_passes_validator           ... ok
test id_type_mapping_unit_table                     ... ok
test git_sha_detected_as_gitoid                     ... ok
test original_scheme_recoverable_from_comment       ... ok
# ... plus all 10 milestone-078 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured
```

## Test contract

A new set of tests in `mikebom-cli/tests/spdx3_conformance.rs` MUST cover (per US1 + US2 + US3 acceptance scenarios):

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `image_tier_with_repo_tags_passes_validator` | US1 §1, SC-001 | FR-002 (image scheme → vocab) + FR-008 |
| `source_tier_in_git_repo_passes_validator` | US1 §2, SC-002 | FR-002 (repo + git schemes → vocab) + FR-004 (gitoid detection) + FR-008 |
| `build_tier_with_subjects_passes_validator` | US1 §3, SC-003 | FR-002 (subject + attestation → vocab) + FR-008 |
| `user_defined_scheme_passes_validator` | US2 §1, SC-004 | FR-003 (user-defined non-vocab → other) + FR-008 |
| `id_type_mapping_unit_table` | (unit) | FR-001 + FR-005 (table-driven; covers every mikebom scheme + edge cases) |
| `git_sha_detected_as_gitoid` | US1 §2 (refined) | FR-004 (gitoid detection regex) |
| `original_scheme_recoverable_from_comment` | US1 §4 + US2 §2, SC-005 | FR-002 + FR-003 (comment-field info preservation) |
| `cdx_byte_identity_preserved` | (regression smoke) | FR-006 (CDX 1.6 untouched) |
| `spdx2_byte_identity_preserved` | (regression smoke) | FR-011 (SPDX 2.3 untouched) |

Note: `cdx_byte_identity_preserved` and `spdx2_byte_identity_preserved` are not new tests — they're existing `cdx_regression` and `spdx_regression` test targets that this milestone MUST NOT cause to require regen. The contract requires them to keep passing without `MIKEBOM_UPDATE_*_GOLDENS` env vars.

## Performance contract

- Mapping function: pure-function with O(1) per-identifier cost. Regex match is O(n) on value length but bounded by 40 chars in practice. Negligible vs. JSON serialization wall-time.
- Integration test wall-time: extends milestone 078's <60s envelope by ~10–15s (4 new tests × ~3s each for fresh-emission + validation). Total <75s.
- Validator wall-time per fixture: <3s per the milestone-078 baseline. New fixtures fit the same envelope.
- Determinism (FR-005): re-running the test against the same inputs produces identical results.
