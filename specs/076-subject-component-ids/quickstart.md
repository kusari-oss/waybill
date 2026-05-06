# Quickstart — milestone 076 subject identifier + per-component identifiers

Five operator-facing recipes covering the milestone's two deliverables: document-level `subject:` identifiers and per-component user-defined identifiers.

## Recipe 1 — Build-tier `subject:` auto-detect

The headline. No flags needed beyond a normal `mikebom trace run` invocation.

```bash
mikebom trace run --signing-key ./signing.key \
    --sbom-output build.cdx.json \
    --attestation-output build.attestation.dsse.json \
    -- cargo install ripgrep
# INFO build-tier auto-detected `subject:sha256:abc1234567890...` from in-toto subject `ripgrep`
```

Inspect the build SBOM:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "attestation")' build.cdx.json
# [
#   {
#     "type": "attestation",
#     "url": "sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab",
#     "comment": "auto-detected from build-tier in-toto subject `ripgrep`"
#   }
# ]
```

The build SBOM body now carries the build-output hash as a first-class `subject:` identifier. External tools can read it without unwrapping the `.attestation.dsse.json` envelope.

For multi-output builds (e.g., `make all`), multiple entries appear:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "attestation") | .url' build.cdx.json
# "sha256:abc1234567890..."
# "sha256:def5678901234..."
# "sha256:fed9876543210..."
```

## Recipe 2 — Cross-tier digest handshake

The end-to-end correlation flow milestone 076 enables. Build a binary, scan the resulting image, then walk the chain by string match alone.

```bash
# Step 1: Source-tier scan
cd ~/projects/my-app
mikebom sbom scan --path . --output source.cdx.json
# (auto-detects repo:git@github.com:acme/my-app.git from milestone 073)

# Step 2: Build-tier scan
mikebom trace run --signing-key ./key.pem \
    --sbom-output build.cdx.json \
    --attestation-output build.attestation.dsse.json \
    -- docker build -t my-app:v1 .
# (auto-detects repo:, git:, AND subject:sha256:<image-digest>)

# Step 3: Image-tier scan
mikebom sbom scan --image my-app:v1 --output image.cdx.json
# (auto-detects image:my-app:v1@sha256:<image-digest>)
```

Now correlate without invoking mikebom:

```bash
# Extract the build's subject hashes
build_subjects=$(jq -r '.metadata.component.externalReferences[]
                          | select(.type == "attestation")
                          | select(.url | startswith("sha256:"))
                          | .url' build.cdx.json)

# Extract the image's identifier digest
image_digest_full=$(jq -r '.metadata.component.externalReferences[]
                              | select(.type == "distribution")
                              | .url' image.cdx.json)
# my-app:v1@sha256:abc1234567890...

# The handshake: build subject's hex equals image digest's hex
echo "Build subjects: $build_subjects"
echo "Image: $image_digest_full"
# → match by string: "sha256:abc1234..." appears in both
```

External SBOM-store tools can do this lookup automatically. The chain `image → build → source` is recoverable by walking identifiers across SBOMs in the store, no mikebom-side coordination required.

## Recipe 3 — Manual `--subject-hash` (source-tier or override)

For source-tier scans that have an externally-known content hash, or for non-sha256 digests, pass the value manually.

```bash
mikebom sbom scan --path . \
    --subject-hash sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab \
    --output source.cdx.json
```

Inspect:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "attestation")' source.cdx.json
# [
#   {
#     "type": "attestation",
#     "url": "sha256:abc1234567890...",
#     "comment": "manual --subject-hash"
#   }
# ]
```

For non-sha256 algos (the auto-detect path skips these per the 2026-05-06 clarification):

```bash
mikebom trace run \
    --subject-hash sha512:def5678901234abc... \
    -- ./build.sh
# (manual sha512 entry rides through; auto-detect of any sha256 entries on the same subject still fires)
```

## Recipe 4 — Per-component user-defined identifiers

Attach internal asset IDs to specific components.

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001" \
    --output out.cdx.json
```

The matching components carry the user-defined identifiers in standards-native CDX `properties[]`:

```bash
jq '.components[] | select(.purl == "pkg:cargo/serde@1.0.0") | .properties' out.cdx.json
# [
#   {"name": "kusari-id", "value": "asset-shared-lib-v2"}
# ]
```

Multiple identifiers per component:

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/foo@1.0.0=kusari-id:asset-foo" \
    --component-id "pkg:cargo/foo@1.0.0=internal-ticket:PROJ-456" \
    --output out.cdx.json

jq '.components[] | select(.purl == "pkg:cargo/foo@1.0.0") | .properties' out.cdx.json
# [
#   {"name": "internal-ticket", "value": "PROJ-456"},
#   {"name": "kusari-id", "value": "asset-foo"}
# ]
# (lexical order by (scheme, value) per research §6)
```

If a selector matches zero components:

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/nonexistent@0.0.0=asset:foo"
# WARN --component-id selector `pkg:cargo/nonexistent@0.0.0` matched zero components; identifier `asset:foo` not attached
```

The scan still exits 0; the SBOM is emitted without the unmatched identifier.

## Recipe 5 — Reading per-component identifiers in each format

Per-component user-defined identifiers ride native fields per format. Operators / external tools read them like this:

### CDX 1.6

```bash
jq '.components[]
      | {purl, identifiers: [
          .properties[]?
          | select(.name | test("^[a-z][a-z0-9_-]*$"))
          | {scheme: .name, value: .value}
        ]}' out.cdx.json
# {
#   "purl": "pkg:cargo/serde@1.0.0",
#   "identifiers": [{"scheme": "kusari-id", "value": "asset-shared-lib-v2"}]
# }
```

### SPDX 2.3

```bash
jq '.packages[]
      | {name, identifiers: [
          .externalRefs[]?
          | select(.referenceCategory == "PERSISTENT-ID")
          | select(.referenceType != "purl")
          | {scheme: .referenceType, value: .referenceLocator}
        ]}' out.spdx.json
# {
#   "name": "serde",
#   "identifiers": [{"scheme": "kusari-id", "value": "asset-shared-lib-v2"}]
# }
```

### SPDX 3

```bash
jq '.elements[]
      | select(.type == "software_Package")
      | {name, identifiers: [
          .externalIdentifier[]?
          | select(.type != "purl")
          | {scheme: .type, value: .identifier}
        ]}' out.spdx3.json
```

All three formats expose the identifiers via existing standards-native fields. No mikebom-specific decoders required.
