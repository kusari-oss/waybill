# Quickstart — milestone 079 SPDX 3 externalIdentifierType conformance

Five operator-facing recipes covering the post-fix wire shape, validator coverage of the new identifier sources, and the per-scheme mapping reference.

## Recipe 1 — Inspect the post-fix wire shape

```bash
mikebom sbom scan --image registry.example.com/img:tag --output out.spdx3.json --format spdx-3-json
jq '.["@graph"][] | select(.type == "ExternalIdentifier") | {
       externalIdentifierType,
       identifier,
       comment
     }' out.spdx3.json
# {
#   "externalIdentifierType": "other",                         ← in vocab (post-fix)
#   "identifier": "registry.example.com/img:tag",
#   "comment": "original-scheme: image"                        ← NEW: original scheme preserved
# }
# {
#   "externalIdentifierType": "packageUrl",                    ← unchanged (vocab-named)
#   "identifier": "pkg:oci/...",
#   "comment": null                                            ← omitted when no info loss
# }
```

Pre-fix, the same SBOM emitted `"externalIdentifierType": "image"` — non-conformant per SPDX 3 controlled vocabulary. Post-fix uses `"other"` with the original `image` scheme name preserved in `comment` for cross-tier correlation tooling.

## Recipe 2 — Verify a freshly-emitted SBOM passes the validator

```bash
# One-time setup (if not already done from milestone 078)
bash scripts/install-spdx3-validate.sh
# → installed to .venv/spdx3-validate/bin/spdx3-validate (pinned 0.0.5)

# Validate any mikebom-emitted SPDX 3 file from any scan tier
.venv/spdx3-validate/bin/spdx3-validate -j out.spdx3.json
echo $?
# 0  ← validator passed; SBOM conforms
```

If you see any `Violation of type ... Core/externalIdentifierType: ... Value is not in {other, cve, ...}` lines, that's a regression — file an issue with the validator's full stderr captured.

## Recipe 3 — Filter cross-tier identifiers by original scheme

Cross-tier correlation tooling (e.g., milestone 072's `verify-binding` / `trace-binding` workflows) recovers the original mikebom scheme from the `comment` field's `original-scheme: ` prefix. Generic spec-aware tooling can still discover identifiers via the conformant `externalIdentifierType` value.

```bash
# Find all identifiers whose original scheme was `subject` (build-tier)
jq '.["@graph"][]
    | select(.type == "ExternalIdentifier")
    | select(.comment // "" | startswith("original-scheme: subject"))
    | .identifier' out.spdx3.json
# "<subject identifier value 1>"
# "<subject identifier value 2>"

# Same query for image-tier identifiers
jq '.["@graph"][]
    | select(.type == "ExternalIdentifier")
    | select(.comment // "" | startswith("original-scheme: image"))
    | .identifier' out.spdx3.json
# "<image identifier value>"
```

The `original-scheme: ` prefix is structured by design (per research §3); downstream tooling can parse it deterministically.

## Recipe 4 — Per-scheme mapping reference

Use this table to predict the SPDX 3 wire shape from any mikebom scheme + value pair:

| Input mikebom scheme | Input value shape | SPDX 3 `externalIdentifierType` | SPDX 3 `comment` |
|---|---|---|---|
| `image` (auto-detect or `--component-id`) | any | `other` | `"original-scheme: image"` |
| `repo` (auto-detect or `--component-id`) | any | `other` | `"original-scheme: repo"` |
| `git` (milestone-074 auto-detect) | always 40-char hex SHA-1 | `gitoid` | (omitted) |
| `git` (`--component-id git=<URL>` or similar) | not a SHA-1 | `other` | `"original-scheme: git"` |
| `subject` (milestone-076 build-tier) | any | `other` | `"original-scheme: subject"` |
| `attestation` (milestone-076 build-tier) | any | `other` | `"original-scheme: attestation"` |
| `--component-id <PURL>=cve:<value>` (vocab-named) | any | `cve` | (omitted) |
| `--component-id <PURL>=cpe23:<value>` (vocab-named) | any | `cpe23` | (omitted) |
| (any other `--component-id <PURL>=<SCHEME>:<VALUE>` whose `<SCHEME>` is in the SPDX 3 vocab) | any | `<SCHEME>` verbatim | (omitted) |
| `--component-id <PURL>=jira:<value>` (non-vocab name) | any | `other` | `"original-scheme: jira"` |
| (any other `--component-id <PURL>=<SCHEME>:<VALUE>` whose `<SCHEME>` is NOT in the vocab; built-ins `repo`/`git`/`image`/`attestation`/`subject` are rejected at parse time) | any | `other` | `"original-scheme: <SCHEME>"` |

The `comment` field is omitted (not emitted as `null`, not emitted as empty string) when no information would be lost.

## Recipe 5 — Pre-PR gate behavior

Identical to milestone 078 — no new local-dev workflow:

```bash
# (optional) install the validator once
bash scripts/install-spdx3-validate.sh

# run the gate
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# >>> cargo +stable clippy --workspace --all-targets -- -D warnings
# >>> cargo +stable test --workspace
# (during cargo test, spdx3_conformance now runs ~17 tests covering
#  every identifier source path; all must report "0 failed")
# >>> all pre-PR checks passed.
```

If you don't have Python configured locally, omit `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` and the conformance test gracefully skips with a clear diagnostic — same milestone-078 behavior.

## What's NOT changed by this milestone

- **CDX 1.6 emission**: byte-identical to alpha.19. The CDX `externalReferences[].type` vocabulary is independent.
- **SPDX 2.3 emission**: byte-identical to alpha.19. The SPDX 2.3 `externalRefs` vocabulary is independent.
- **CLI flag set**: no new flags. Existing flags from milestones 073–078 unchanged.
- **`SchemeName` / `Identifier` internal types**: preserved verbatim. The mapping happens at SPDX 3 emission time only.
- **The 9 SPDX 3 source-tier ecosystem goldens** (`apk`/`cargo`/`deb`/`gem`/`golang`/`maven`/`npm`/`pip`/`rpm`): byte-identical post-fix because they don't exercise auto-detected, build-tier, or user-defined identifiers.
- **Validator install + version pin**: still `spdx3-validate==0.0.5` per milestone 078. No bump.
