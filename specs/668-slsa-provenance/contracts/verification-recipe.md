# Contract: verification recipe shape (FR-009 + FR-010)

**Feature**: `668-slsa-provenance` | **Applies to**: `docs/verifying-releases.md`

The verification-recipe documentation is a user-facing contract with downstream consumers. It MUST satisfy these behavioral constraints to close the value chain from "we emit provenance" to "consumers verify it."

## Interface

### Input (per recipe)

- **Artifact type**: one of {tarball, OCI image, source SBOM sidecar}
- **Artifact source**: GitHub Releases page URL, `ghcr.io/kusari-oss/waybill:<tag>`, or a mirrored URL
- **Consumer environment**: any OS with the `gh` CLI ≥ 2.49 installed

### Output

- A copy-paste-ready shell one-liner that produces a green "verification succeeded" result, printing (at minimum) the source commit SHA and workflow-run URL that produced the artifact.

## Behavioral contracts

### C-1: Copy-paste success within 5 minutes (SC-004)

A first-time consumer following the recipe MUST reach a green verification result within 5 minutes measured from landing on the docs page to the terminal `gh attestation verify` exit code 0. Includes install-time for `gh` if absent. Verified by a non-author engineer time-boxing the recipe walk-through on a fresh workstation.

### C-2: One recipe per artifact type (FR-009)

The docs MUST contain AT LEAST three recipes — one for each artifact type in FR-001/FR-002/FR-003:

1. **Tarball recipe** — download from GitHub Releases + run `gh attestation verify <tarball>`
2. **OCI image recipe** — `gh attestation verify oci://ghcr.io/kusari-oss/waybill:<tag>`
3. **Source SBOM recipe** — download source SBOM sidecar + run `gh attestation verify <sbom-file>`

Optional additional recipes: cosign-based alternative (per R2 alternative), Rekor-direct lookup (for consumers who bypass `gh`).

### C-3: Mirrored-artifact recipe (FR-010)

The docs MUST include a recipe for verifying an artifact that has been mirrored OUT of GitHub Releases / GHCR into a third-party registry (Artifactory, Nexus, Harbor, etc.). This recipe MUST use the transferable Sigstore bundle file rather than relying on GitHub's attestation API.

Recipe shape:
```bash
# On the mirror-publish side (once, at release-publish time):
gh attestation download <artifact> --repo kusari-oss/waybill --output bundle.jsonl

# On the consumer side (any time later, offline or online):
gh attestation verify --bundle bundle.jsonl <artifact>
```

### C-4: Failure-mode documentation

The docs MUST cover the following failure modes with example error messages and remediation guidance:

- Verification against a tampered artifact (any-byte-flipped)
- Verification against a release predating m668's landing (historical releases without provenance)
- Verification when the local `gh` CLI is out of date
- Verification when GitHub's attestation API is temporarily unreachable (bundle-file recipe from C-3 is the fallback)

## Structure

The docs page MUST have this section shape (heading levels flexible):

1. **What this page covers** — one-paragraph orientation
2. **Prerequisites** — `gh` CLI version, network access assumptions
3. **Recipe 1: verify a downloaded tarball** — copy-paste one-liner + explanation of output
4. **Recipe 2: verify an OCI image** — same shape
5. **Recipe 3: verify a source SBOM sidecar** — same shape
6. **Recipe 4: verify a mirrored artifact via bundle file** (C-3)
7. **Troubleshooting** (C-4)
8. **Provenance predicate reference** — link to `specs/668-slsa-provenance/data-model.md` for consumers who want to inspect the raw predicate

## Non-contracts

- The docs do NOT need to teach SLSA at first-principles level. Link to slsa.dev's own docs for readers wanting the framework tutorial.
- The docs do NOT need to cover consumers using `cosign verify-attestation` in detail — a brief pointer is sufficient; cosign has its own docs.
- The docs do NOT need to cover in-house / self-hosted-Rekor scenarios. Air-gapped consumers use the bundle-file recipe from C-3.

## Discoverability

The docs page MUST be linked from:

- The waybill README.md's "Security" or "Verification" section (whichever exists post-merge; if neither, add a "Verifying releases" section)
- Every GitHub release's notes as a one-line "Verify this release: <docs URL>" pointer

This satisfies FR-009's "discoverable location" requirement.
