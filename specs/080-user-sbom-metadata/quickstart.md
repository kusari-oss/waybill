# Quickstart — milestone 080 user-provided SBOM metadata

Six operator-facing recipes covering the post-fix CLI flags, the per-format wire shape, and the migration path away from `jq` post-processing.

## Recipe 1 — Replace the CNCF-style `jq` recipe

**Before** (the workflow in issue #94):
```bash
mikebom sbom scan --path . --output input.spdx.json --format spdx-2.3-json
jq --arg owner "$OWNER" --arg repo "$REPO" --arg tag "$TAG" \
   --arg generatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
   '.creationInfo.creators += ["Tool: cncf-automation-sbom-generator"] |
    .creationInfo.comment = "SBOM for CNCF project: \($owner)/\($repo)@\($tag)"' \
   input.spdx.json > output.spdx.json
```

Each additional output format (CDX 1.6, SPDX 3) needs its own format-specific `jq` invocation — three different shape edits for the same conceptual operation.

**After** (post-fix native CLI):
```bash
mikebom sbom scan --path . \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=out.cdx.json \
  --output spdx-2.3-json=out.spdx.json \
  --output spdx-3-json=out.spdx3.json \
  --creator "Tool: cncf-automation-sbom-generator" \
  --metadata-comment "SBOM for CNCF project: $OWNER/$REPO@$TAG"
```

Single invocation. Three formats. Each operator-supplied value lands at the standards-native field in every format. No post-processing.

## Recipe 2 — Add a single creator

```bash
mikebom sbom scan --path . --creator "Tool: my-pipeline" --output mikebom.cdx.json
jq '.metadata.tools' mikebom.cdx.json
# {
#   "components": [
#     { "name": "mikebom", "version": "0.1.0-alpha.20", ... },   ← auto-populated
#     { "name": "my-pipeline", "type": "application" }            ← --creator "Tool: my-pipeline"
#   ]
# }
```

For SPDX 2.3:
```bash
mikebom sbom scan --path . --creator "Tool: my-pipeline" --format spdx-2.3-json
jq '.creationInfo.creators' mikebom.spdx.json
# [
#   "Tool: mikebom-0.1.0-alpha.20",
#   "Tool: my-pipeline"
# ]
```

## Recipe 3 — Add multi-annotation context with positional pairing

```bash
mikebom sbom scan --path . \
  --annotator "Tool: security-scanner" \
  --annotation-comment "Reviewed for CVE-2024-1234 exposure" \
  --annotator "Organization: SecOps" \
  --annotation-comment "PCI compliance scan complete" \
  --output mikebom.spdx.json --format spdx-2.3-json

jq '.annotations' mikebom.spdx.json
# [
#   {
#     "annotator": "Tool: security-scanner",
#     "annotationDate": "<emission-time>",
#     "annotationType": "OTHER",
#     "comment": "Reviewed for CVE-2024-1234 exposure"
#   },
#   {
#     "annotator": "Organization: SecOps",
#     "annotationDate": "<emission-time>",
#     "annotationType": "OTHER",
#     "comment": "PCI compliance scan complete"
#   }
# ]
```

The positional pairing rule (per spec Q1 clarification): each `--annotator` MUST be immediately followed by exactly one `--annotation-comment`. Out-of-order forms fail at parse time with `"--annotator (count=2) must be paired 1:1 with --annotation-comment (count=1); each --annotator MUST be immediately followed by exactly one --annotation-comment"`.

## Recipe 4 — Use a sidecar `--metadata-file`

```bash
cat > meta.json <<'EOF'
{
  "creators": [
    "Tool: my-pipeline",
    "Organization: ACME Corp",
    "Person: Alice"
  ],
  "annotators": [
    {"type_name": "Tool: reviewer", "comment": "Approved 2026-05-07"},
    {"type_name": "Organization: SecOps", "comment": "PCI scan complete"}
  ],
  "metadata_comment": "Release v1.0.0",
  "scan_target_name": "myproject"
}
EOF

mikebom sbom scan --path . --metadata-file meta.json --format spdx-2.3-json
```

The file's content is equivalent to passing the corresponding flags individually. Pipelines that already manage structured metadata (e.g., a release manifest in CI) prefer the file form over flag soup.

**Mixing file + flags** (per FR-006): array fields (`creators`, `annotators`) merge additively (file first, then flags). Single-valued fields (`metadata_comment`, `scan_target_name`) MUST NOT appear in BOTH file AND flags — fail with a clear conflict error.

## Recipe 5 — Inspect the post-fix wire shape across formats

Every operator-supplied flag lands at the standards-native field in each format:

| Operator input | CDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| `--creator "Tool: T"` | `metadata.tools.components[]` | `creationInfo.creators[]` (`Tool: T`) | `Tool` element in `@graph` |
| `--creator "Organization: O"` (1st) | `metadata.manufacturer = {name: O}` | `creationInfo.creators[]` (`Organization: O`) | `Organization` element |
| `--creator "Person: P"` | `metadata.authors[]` | `creationInfo.creators[]` (`Person: P`) | `Person` element |
| `--metadata-comment X` | `bom.annotations[]` (annotator=mikebom-contributors, text=X) | `creationInfo.comment = X` | `Annotation` of type OTHER, statement=X |
| `--annotator "Tool: T" --annotation-comment Y` | `bom.annotations[]` (annotator=T, text=Y) | `annotations[]` (annotator=`Tool: T`, comment=Y) | `Annotation` of type OTHER, annotator references the Tool Agent |
| `--scan-target-name N` | `metadata.component.name = N` (interacts with `--root-name`) | document `name = N` | `software_Sbom.name = N` |

Verify by running:
```bash
jq '.metadata.tools.components[] | {name}, .metadata.manufacturer, .metadata.authors, .annotations' out.cdx.json
jq '.creationInfo, .annotations' out.spdx.json
jq '.["@graph"][] | select(.type == "Tool" or .type == "Organization" or .type == "Person" or .type == "Annotation")' out.spdx3.json
```

## Recipe 6 — Pre-PR gate behavior

Identical to milestone 078 / 079 — no new local-dev workflow:

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# >>> cargo +stable clippy --workspace --all-targets -- -D warnings
# >>> cargo +stable test --workspace
# (during cargo test, sbom_user_metadata runs ~17 tests covering
#  every flag and edge case; the conformance gate continues to verify
#  SPDX 3 SBOMs with the new metadata pass spdx3-validate zero-violation)
# >>> all pre-PR checks passed.
```

The new tests inherit milestone 078's `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` env-var hook for graceful-skip-when-missing on local dev without Python.

## What's NOT changed by this milestone

- **`mikebom sbom enrich`**: continues to handle per-component metadata edits via JSON Patch. Document-level edits move to native flags; per-component edits stay on `enrich`.
- **Auto-populated mikebom entry**: every emitted SBOM still carries the mikebom auto-populated tool/creator entry. The new flags add OPERATOR entries on top — they don't replace or hide mikebom's own.
- **Validator pin**: `spdx3-validate==0.0.5` per milestone 078. No bump.
- **Pre-existing operator workflows**: invocations without any of the new flags produce byte-identical SBOMs to alpha.20. Adoption is fully opt-in.
- **Signing-side verification**: this milestone does NOT verify that operator-supplied creator strings match a known organization or OIDC identity. That's a sigstore-style concern tracked separately.
