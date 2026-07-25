# Quickstart

Stable recipes come first — they produce CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1
JSON SBOMs, work on any OS, and need no special privileges. Trace-mode
(experimental, Linux only) follows at the bottom.

Prereqs: [`waybill` installed](installation.md) and on `$PATH`.

---

## Recipe 1 — Scan a source tree

Point at any directory that contains lockfiles or manifests. Works on any OS.

```bash
waybill sbom scan --path ./my-project --output project.cdx.json --json
```

Waybill reads every supported lockfile (`Cargo.lock`, `package-lock.json`,
`pnpm-lock.yaml`, `go.mod` + `go.sum`, `Gemfile.lock`, `pom.xml`,
`poetry.lock`, `Pipfile.lock`, `requirements.txt`) plus Maven JAR
`META-INF/maven/.../pom.xml`, per-module Go `.mod` files from the module
cache if present, and produces a CycloneDX with:

- SHA-256 content hashes on every component
- Real `dependsOn` edges (not a flat fan-out)
- Evidence blocks pointing back to the file that identified each component
- Strict PURL encoding round-trippable through `packageurl-python`

For richer Go dep graphs, run `go mod download` (or let `go build` populate
`$GOMODCACHE`) before the scan — per-module `.mod` files let Waybill walk the
transitive require graph.

See [CLI reference: `waybill sbom scan`](cli-reference.md) for the full flag
list.

---

## Recipe 2 — Scan a container image

Works on any OS. No privilege, no eBPF.

```bash
waybill sbom scan --image alpine:3.19 --output alpine.cdx.json --json
```

For OCI references Waybill checks the local docker daemon's cache first then
falls back to a registry pull on miss. Pass a `docker save` tarball if you'd
rather feed bytes directly:

```bash
docker save alpine:3.19 -o alpine.tar
waybill sbom scan --image alpine.tar --output alpine.cdx.json --json
```

`--image` extracts the layers (honouring OCI whiteouts), auto-reads
`<rootfs>/etc/os-release` for `ID` + `VERSION_ID` (feeding the
`distro=<namespace>-<version>` PURL qualifier — `distro=debian-12`,
`distro=alpine-3.19`), reads installed-package databases
(`/var/lib/dpkg/status` for Debian and derivatives, `/lib/apk/db/installed`
for Alpine, `rpmdb.sqlite` for RPM-based images), and emits a CycloneDX SBOM
with a real dependency graph from the db's `Depends:` fields.

`--json` prints a summary to stdout:

```json
{
  "components": 15,
  "relationships": 6,
  "generation_context": "container-image-scan",
  "target_name": "alpine:3.19"
}
```

For Debian / Ubuntu / Alpine / RPM-based images the scanner also produces
per-file SHA-256 evidence (the `evidence.occurrences[]` block) so every
component carries byte-level tamper detection. Pass `--no-deep-hash` to skip
this on very large images; `--no-package-db` to fall back to artifact-file-
only scanning.

Pass `--image-src remote` to force a fresh registry fetch (skip the local
docker cache):

```bash
waybill sbom scan --image alpine:3.19 --image-src remote --output alpine.cdx.json
```

### Authenticated registries

Credentials come from `~/.docker/config.json` (both `Bearer`-style —
Docker Hub, GHCR, gcr.io — and `Basic`-style — AWS ECR — challenges).
Run `docker login <registry>` (or
`aws ecr get-login-password | docker login --username AWS …` for ECR)
once and Waybill picks up the credentials.

### OCI Referrers API (milestone 186 / #442)

When a registry publishes a pre-generated SBOM via the OCI Distribution
Spec v1.1 Referrers API (e.g., `docker buildx --sbom=true` output), the
`--sbom-source` flag lets Waybill prefer it over re-scanning the image
bytes:

```bash
# Prefer a referrer if published, fall through to scan otherwise.
waybill sbom scan --image ghcr.io/example/app:v1 \
    --sbom-source either \
    --format cyclonedx-json --output app.cdx.json

# Compliance: require a matching upstream SBOM (fail if absent).
waybill sbom scan --image ghcr.io/example/app:v1 \
    --sbom-source referrer \
    --format cyclonedx-json --output app.cdx.json

# Default (`scan`) preserves pre-m186 behavior byte-identically —
# no network activity on the Referrers endpoint.
waybill sbom scan --image ghcr.io/example/app:v1 \
    --format cyclonedx-json --output app.cdx.json
```

Referrer content is emitted byte-identically to preserve any
upstream signer's Cosign / in-toto DSSE contracts. Full flag
reference: `specs/186-oci-referrers-sbom/quickstart.md`.

See [Architecture: scanning](../architecture/scanning.md) for the ecosystem
walker design.

---

## Recipe 3 — Trace a build (Linux only, experimental)

> **Status: experimental.** Linux-only. Adds ~2-3× wall-clock overhead on
> syscall-heavy builds; requires CAP_BPF + CAP_PERFMON; coverage gaps on
> `openat2` / `io_uring`. Most users should stick with the scan recipes
> above. The trace-mode pipeline exists for workflows that need the SBOM to
> be provably bound to a specific build event.

Trace `cargo install ripgrep` end-to-end, produce a signed attestation, then
derive a CycloneDX SBOM from that attestation:

```bash
waybill trace run \
  --sbom-output ripgrep.cdx.json \
  --attestation-output ripgrep.attestation.json \
  --signing-key ./signing.key \
  --auto-dirs \
  -- cargo install ripgrep
```

To re-derive the SBOM later (or after enriching with different flags), use
`waybill sbom verify` with the attestation as input. On macOS, run
trace-mode inside the `waybill-dev` container or a Lima VM — see
[installation](installation.md).

---

## Recipe 4 — Assert SBOM type with `--sbom-type`

When your pipeline knows the SBOM should be classified as a single CISA SBOM
Type regardless of Waybill's per-component auto-detection, override at the
document level:

```bash
waybill sbom scan --path . \
    --sbom-type build \
    --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
    --output cyclonedx-json=out.cdx.json \
    --output spdx-2.3-json=out.spdx.json \
    --output spdx-3-json=out.spdx3.json
```

After the override, all three formats collapse the document-level signal to
`["build"]`:

```bash
jq '.metadata.lifecycles' out.cdx.json
# [{"phase": "build"}]

jq -r '.creationInfo.comment' out.spdx.json
# "Scope: ... Observed lifecycle phases: build. ..."

jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# ["build"]
```

Per-component `waybill:sbom-tier` annotations preserve their auto-detected
values — the override is a CLAIM about document-level type, not a rewrite of
per-component lineage.

See [SBOM types](../reference/sbom-types.md) for the full per-format field
positions and four-column CISA equivalence reference.

---

## Recipe 5 — Override the SBOM root component name

When scanning an arbitrary directory whose basename doesn't reflect the
operator-meaningful project identity, override `metadata.component.name`
(and optionally `version`):

```bash
waybill sbom scan --path /tmp/extracted \
    --root-name acme-platform \
    --root-version 2.4.1 \
    --output platform.cdx.json
```

Output:

```bash
jq '.metadata.component | {name, version}' platform.cdx.json
# {
#   "name": "acme-platform",
#   "version": "2.4.1"
# }
```

When this flag is set on a manifest-driven scan (Cargo, npm, pip, gem,
Maven, Go), the manifest-derived main-module component is dropped entirely
from the emitted SBOM (clean replacement).

See [CLI reference: `--root-name`](cli-reference.md) for the validation
rules.

---

## Recipe 6 — Attach a user-defined component identifier

Attach a stable identity (e.g., your internal asset-management ID) to a
specific component in the emitted SBOM:

```bash
waybill sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --output project.cdx.json
```

If a selector PURL matches multiple components (same PURL across different
bom-ref values), the identifier is attached to ALL matching components. If
a selector matches zero components, the scan logs a warning and continues.

CDX 1.6 lands the identifier in `components[].properties[]` as
`waybill:component-identifier`; SPDX 2.3 lands it as a per-package
`Annotation`; SPDX 3 carries it natively in
`software_Package.externalIdentifier[]`.

See [Identifiers](../reference/identifiers.md) for the full per-format
carrier table and decode recipes.

---

## Recipe 7 — Tag a Kubernetes workload (cluster, namespace, pod)

When SBOMs are generated from Kubernetes cluster scans, downstream consumers
(Dependency-Track, SeeBOM, in-house vulnerability dashboards) need to know
*which cluster*, *which namespace*, and *which workload* an SBOM belongs to.
Without this metadata embedded in the SBOM itself, the link between "this
SBOM" and "this running workload" is lost once the file leaves the scanning
context.

Waybill doesn't ship dedicated `--cluster-id` / `--namespace` flags. Use the
existing `--id <scheme>=<value>` flag (repeatable) to encode K8s workload
identity:

```bash
waybill sbom scan --image ghcr.io/example/webapp:1.25 \
    --id k8s_cluster=prod-us-east \
    --id k8s_namespace=production \
    --id k8s_workload_name=webapp-v2 \
    --id k8s_workload_kind=Deployment \
    --id k8s_workload_uid=abc-123-def \
    --output webapp-prod.cdx.json
```

The `k8s_*` scheme prefix is a convention — pick any naming pattern that fits
your downstream consumer's expectations. The scheme name regex is
`^[a-z][a-z0-9_-]*$`; `repo`, `git`, `image`, `attestation`, `subject` are
reserved for the dedicated flags.

### Where the values land per format

| Format | Carrier |
|---|---|
| CycloneDX 1.6 | `metadata.annotations[].text` inside the document-level `waybill:identifiers` envelope |
| SPDX 2.3 | `annotations[]` at document level inside the `MikebomAnnotationCommentV1` envelope |
| SPDX 3.0.1 | `Element.externalIdentifier[]` (native carrier, one entry per `--id`) |

See [Identifiers](../reference/identifiers.md) for the full per-format carrier
table and decode recipes.

### Driving the flags from a Kubernetes operator

The values are typically derived from the `Pod` object's metadata fields and
the `ownerReferences[]` chain. From a CronJob or operator that scans pods:

```bash
# Inside a pod's scan loop, with $POD, $NAMESPACE, $CLUSTER pre-populated:
WORKLOAD_KIND=$(kubectl get pod "$POD" -n "$NAMESPACE" \
    -o jsonpath='{.metadata.ownerReferences[0].kind}')
WORKLOAD_NAME=$(kubectl get pod "$POD" -n "$NAMESPACE" \
    -o jsonpath='{.metadata.ownerReferences[0].name}')
WORKLOAD_UID=$(kubectl get pod "$POD" -n "$NAMESPACE" \
    -o jsonpath='{.metadata.ownerReferences[0].uid}')

waybill sbom scan --image "$(kubectl get pod "$POD" -n "$NAMESPACE" \
    -o jsonpath='{.spec.containers[0].image}')" \
    --id k8s_cluster="$CLUSTER" \
    --id k8s_namespace="$NAMESPACE" \
    --id k8s_workload_kind="$WORKLOAD_KIND" \
    --id k8s_workload_name="$WORKLOAD_NAME" \
    --id k8s_workload_uid="$WORKLOAD_UID" \
    --output "/sboms/${WORKLOAD_NAME}.cdx.json"
```

For deployments-from-CronJob patterns where credentials arrive via a
mounted `imagePullSecret`, see also
[`--registry-credentials-dir`](cli-reference.md#--registry-credentials-dir-path).

### Naming tips

- **Stable across pod restarts**: prefer `workload_name` + `workload_kind`
  (Deployment/StatefulSet/...) over the pod's own ephemeral name.
- **Stable across deployments**: prefer `workload_uid` (the K8s UID of the
  owning workload) as the canonical identifier. Pod UIDs change per restart;
  Deployment UIDs do not.
- **Multi-cluster**: a `cluster_id` should disambiguate across environments
  (e.g., `prod-us-east`, `staging-eu-central`). The K8s API doesn't expose a
  built-in cluster name — your scanning operator must inject it.

---

## Recipe 8 — Use a metadata sidecar file

Centralize creator/annotator/comment metadata in a JSON sidecar instead of
passing dozens of CLI flags:

```bash
cat >metadata.json <<'JSON'
{
  "creators": [
    "Tool: ci-scanner@1.4.0",
    "Organization: Acme Corp"
  ],
  "annotators": [
    {"type_name": "Person: alice@acme.example", "comment": "Reviewed for SOC2"}
  ],
  "metadata_comment": "Generated for SOC2 audit 2026-Q2",
  "scan_target_name": "acme-platform"
}
JSON

waybill sbom scan --path . --metadata-file metadata.json --output project.cdx.json
```

`deny_unknown_fields` applies. Array fields merge additively with their flag
counterparts (file values come first); single-valued fields fail with a
conflict error if specified in both.

See [CLI reference: `--metadata-file`](cli-reference.md) for the schema.

---

## Recipe 9 — Verify a signed DSSE attestation

Works on any OS. Accepts DSSE envelopes produced by Waybill, witness, or any
other SBOMit-compliant tool.

```bash
waybill sbom verify attest.dsse.json \
  --public-key signer.pub \
  --expected-subject ./my-binary
```

Output (success):

```text
PASS — verified with public_key sha256:...  subject digest matches on-disk binary.
```

For keyless verification, pass `--identity 'user@example.com'` or a glob
instead of `--public-key`.

See [CLI reference: `waybill sbom verify`](cli-reference.md) for the full
flag set including `--layout` (in-toto policy enforcement) and the
`FailureMode` exit-code contract.

---

## Recipe 10 — Verify a cross-tier binding

When you have both a source-tier SBOM and an image-tier SBOM that was
emitted with `--bind-to-source`, verify that the image-tier per-component
binding annotations match the recompute against the source SBOM:

```bash
waybill sbom verify-binding \
    --image-sbom image.cdx.json \
    --source-sbom source.cdx.json \
    --format json
```

Exits non-zero on any verification failure. Use `--format json` to feed CI
pipelines.

For triage of an unknown image-tier component, use the informational
counterpart:

```bash
waybill sbom trace-binding \
    --component-purl "pkg:cargo/serde@1.0.0" \
    --image-sbom image.cdx.json \
    --candidate-sources-dir ./source-sboms
```

`trace-binding` always exits 0 — it's informational, not validating.

See [Cross-tier binding](../reference/cross-tier-binding.md) for the
binding-hash algorithm and per-format carrier shapes.

---

## Recipe 11 — Generate an in-toto layout

```bash
waybill policy init --functionary-key ci.pub --step-name build --output layout.json
waybill sbom verify attest.dsse.json --layout layout.json
```

Layouts are standard in-toto — any in-toto-aware verifier accepts them. Use
`--expires <DURATION>` to control the validity window (default `1y`).

---

## Recipe 12 — Scan a package cache

Treats cached bytes as present-on-disk; useful for CI cache audits.

```bash
waybill sbom scan --path ~/.cargo/registry/cache --output cargo.cdx.json
```

---

## Recipe 13 — Scan a Maven fat-jar and see shaded ancestors

With feature 009 the SBOM emits one nested component per shade-relocated
ancestor whose bytecode is actually in the JAR.

```bash
waybill sbom scan --path ./target/ --output app.cdx.json

jq '
  .components[]
  | select(.purl | test("pkg:maven/"))
  | .components // []
  | map(select(.properties // [] | any(.name == "waybill:shade-relocation" and .value == "true")))
  | map({purl, bom_ref: ."bom-ref"})
' app.cdx.json
```

---

## Recipe 14 — Enrich an SBOM with an RFC 6902 JSON Patch

Each patch is recorded as a `waybill:enrichment-patch[N]` property on the
BOM metadata so the provenance of every change survives.

```bash
waybill sbom enrich project.cdx.json \
  --patch add-supplier.json --author you@example.com
```

---

**Common flags** across every `sbom *` subcommand: `--offline`,
`--exclude-scope`, `--include-declared-deps`,
`--include-legacy-rpmdb` (env: `WAYBILL_INCLUDE_LEGACY_RPMDB=1`).
See the [CLI reference](cli-reference.md) for the full per-flag
reference and `waybill sbom <verb> --help` for the canonical source.

---

## What's next

- **Find a flag you need?** See [CLI reference](cli-reference.md).
- **Curious why the SBOM looks the way it does?** See
  [Architecture overview](../architecture/overview.md).
- **Running into an unfamiliar ecosystem?** See [Ecosystems](../ecosystems.md).
- **Need cross-tier binding details?** See
  [Cross-tier binding](../reference/cross-tier-binding.md).
- **Identifier model questions?** See [Identifiers](../reference/identifiers.md).
- **SBOM-type signaling?** See [SBOM types](../reference/sbom-types.md).
