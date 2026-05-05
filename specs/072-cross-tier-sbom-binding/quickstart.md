# Quickstart — milestone 072 cross-tier SBOM binding

Five operator-facing recipes against the milestone-072 build of mikebom. Each is runnable end-to-end and demonstrates one concrete outcome from the spec.

## Recipe 1 — Generate + verify a clean source ↔ image binding

The happy path: source SBOM produced from the project repo, image SBOM produced from the image with `--bind-to-source` pointing at the source SBOM, then verified.

```bash
# Step 1: source-tier SBOM
mikebom sbom scan --path ./my-project \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/foo-source.cdx.json

# Step 2: image-tier SBOM, bound to the source SBOM
mikebom sbom scan --image my-registry/foo:v1.0 \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/foo-image.cdx.json \
    --bind-to-source /tmp/foo-source.cdx.json

# Step 3: verify the binding
mikebom sbom verify-binding \
    --image-sbom /tmp/foo-image.cdx.json \
    --source-sbom /tmp/foo-source.cdx.json \
    --format json
```

Expected output (truncated):

```json
{
  "summary": {
    "components_checked": 12,
    "verified": 11,
    "weak": 1,
    "unknown": 0,
    "verification_failures": 0
  },
  "rows": [
    { "purl": "pkg:golang/example.com/foo@v1.0.0",
      "bom_ref": "...",
      "strength": "verified",
      "binding_hash": "a1b2c3..." },
    ...
  ]
}
```

Exit code: `0` (no verification failures).

## Recipe 2 — Detect a wrong source SBOM (different commit)

Same as Recipe 1 but pass a source SBOM from a different commit:

```bash
mikebom sbom verify-binding \
    --image-sbom /tmp/foo-image.cdx.json \
    --source-sbom /tmp/foo-source-WRONG-COMMIT.cdx.json \
    --format json
```

Expected output:

```json
{
  "summary": {
    "components_checked": 12,
    "verified": 0,
    "weak": 0,
    "unknown": 0,
    "verification_failures": 12
  },
  "rows": [
    { "purl": "pkg:golang/example.com/foo@v1.0.0",
      "bom_ref": "...",
      "strength": "unknown",
      "reason": "verification-failed",
      "asserted_hash": "a1b2c3...",
      "recomputed_hash": "ZZZZZZ..." },
    ...
  ]
}
```

Exit code: non-zero (per FR-005). Each row's `recomputed_hash` differs from `asserted_hash` because the wrong source SBOM has a different VCS commit; the layered hash algorithm catches this exactly.

## Recipe 3 — VEX propagation in `caveated` mode (default)

Take a source-tier SBOM with a `not_affected` VEX statement, propagate to image-tier where the image has both a verified-bound instance AND an unbound instance of the same PURL.

```bash
# Source-tier SBOM with OpenVEX sidecar containing not_affected statement.
ls /tmp/source-sbom-and-vex/
# foo-source.cdx.json
# foo-source.openvex.json

# Image-tier SBOM produced with --bind-to-source.
ls /tmp/image-sbom/
# foo-image.cdx.json   (bound where possible)

# Propagate VEX in caveated mode (default in milestone 072).
mikebom sbom enrich \
    /tmp/image-sbom/foo-image.cdx.json \
    --vex-overrides /tmp/source-sbom-and-vex/foo-source.openvex.json \
    --output /tmp/image-sbom/foo-image-enriched.cdx.json \
    --author "ci@example.org"
# (note: --vex-propagation-mode caveated is the default; explicit form is shown
#  in Recipe 4)
```

Expected behavior:

- Verified-bound instance receives the `not_affected` statement cleanly (no caveat).
- Unbound instance receives the statement WITH a `mikebom:vex-binding-status: unverified` caveat AND the OpenVEX `justification` field is preserved unchanged.
- Aggregate per-PURL VEX state (per the C-3 aggregation rule in `contracts/openvex-instance-identifiers.md`) reports `affected` because the unbound instance defaults to "could be affected".

## Recipe 4 — `strict` mode refuses unverified propagation

Same input as Recipe 3, but request strict mode:

```bash
mikebom sbom enrich \
    /tmp/image-sbom/foo-image.cdx.json \
    --vex-overrides /tmp/source-sbom-and-vex/foo-source.openvex.json \
    --vex-propagation-mode strict \
    --output /tmp/image-sbom/foo-image-strict.cdx.json \
    --author "ci@example.org"
```

Expected behavior:

- The verified-bound instance receives the `not_affected` statement.
- The unbound instance gets NO statement; instead, a refusal-rationale annotation is written to the target SBOM under `mikebom:enrichment-patch[N]` listing the (vulnerability, instance) pair that was refused and why.
- Exit code: non-zero (per VR-006), so CI pipelines can gate on strict-mode propagation outcomes.

## Recipe 5 — The worked example (US2 AS#4 / SC-003)

The user's specific worry, end-to-end: a Go networking CVE marked `not_affected` in source because the project doesn't call the vulnerable function, but the image starts a server from non-project code that DOES call it.

```bash
# Source-tier SBOM and OpenVEX sidecar.
# - SBOM contains pkg:golang/golang.org/x/net@v0.28.0 as a transitive dep.
# - OpenVEX statement: { vulnerability: CVE-2024-12345, status: not_affected,
#                        justification: vulnerable_code_not_present }
ls /tmp/proj-source/
# proj-source.cdx.json
# proj-source.openvex.json

# Image-tier SBOM. Two instances of pkg:golang/golang.org/x/net@v0.28.0:
#   instance A: bom-ref `golang-net-from-foo` — bound to source SBOM (verified).
#   instance B: bom-ref `golang-net-from-baselayer-server` — UNBOUND
#     (came from a base-layer system binary mikebom traced no source SBOM for).
ls /tmp/proj-image/
# proj-image.cdx.json

# Propagate in default caveated mode.
mikebom sbom enrich \
    /tmp/proj-image/proj-image.cdx.json \
    --vex-overrides /tmp/proj-source/proj-source.openvex.json \
    --output /tmp/proj-image/proj-image-enriched.cdx.json \
    --author "ci@example.org"

# Inspect aggregate VEX state for the CVE.
jq '.vulnerabilities[] | select(.id == "CVE-2024-12345")' \
    /tmp/proj-image/proj-image-enriched.cdx.json
```

Expected aggregate output:

```json
{
  "id": "CVE-2024-12345",
  "ratings": [...],
  "affects": [
    { "ref": "golang-net-from-foo", "status": "not_affected",
      "justification": "vulnerable_code_not_present",
      "mikebom:vex-binding-status": "verified" },
    { "ref": "golang-net-from-baselayer-server", "status": "affected",
      "mikebom:vex-binding-status": "unverified",
      "rationale": "no source-tier SBOM bound to this instance; default state is `could be affected` per per-instance VEX rule" }
  ],
  "aggregate_status": "affected",
  "aggregate_rationale": "1 of 2 instances is affected; not_affected applies to instance golang-net-from-foo only"
}
```

This is the specific outcome SC-003 / US2 AS#4 demand: the `not_affected` source-tier VEX correctly applies to the bound instance without masking the real `affected` status of the unbound instance.

## Recipe 6 — Operator triage with `trace-binding`

When an operator wants to know "which source-tier SBOM (if any) corresponds to this image-tier component?":

```bash
mikebom sbom trace-binding \
    --component-purl 'pkg:golang/golang.org/x/net@v0.28.0' \
    --image-sbom /tmp/proj-image/proj-image.cdx.json \
    --candidate-sources-dir /tmp/source-sboms/ \
    --format json
```

Expected output:

```json
{
  "component_purl": "pkg:golang/golang.org/x/net@v0.28.0",
  "instances": [
    { "bom_ref": "golang-net-from-foo",
      "binding": { "strength": "verified",
                   "source_doc_id": { "sha256": "e5f6...",
                                      "iri": "/tmp/source-sboms/proj-source.cdx.json" },
                   "binding_hash": "a1b2c3..." },
      "audit_summary": "instance bound to proj-source.cdx.json (verified)" },
    { "bom_ref": "golang-net-from-baselayer-server",
      "binding": { "strength": "unknown",
                   "reason": "base-layer-system-package",
                   "source_doc_id": null,
                   "binding_hash": null },
      "audit_summary": "instance from base-layer; no candidate source SBOM matched" }
  ]
}
```

Exit code: `0` (the command is informational, not validating).
