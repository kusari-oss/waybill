# Research: OCI Referrers API SBOM discovery (m186)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decisions

### Decision 1 — Referrers endpoint URL shape + response type

**Decision**: Query `/v2/<repo>/referrers/<digest>` per OCI Distribution Spec v1.1 §Referrers. The `<digest>` is the resolved SINGLE-PLATFORM manifest digest, not the index digest (for multi-arch images). The response body is an OCI image index (`application/vnd.oci.image.index.v1+json`) containing zero or more descriptors that reference the target digest via their `subject` field. Deserialize the response body as `oci_spec::image::ImageIndex` — the same type m031 already uses for manifest-list resolution.

**Rationale**:
- The Referrers endpoint returns an ImageIndex-shaped response per Distribution Spec v1.1 §Referrers Sample. Reusing `oci_spec::image::ImageIndex` eliminates the need for a bespoke parser and matches the existing `fetch_manifest` code shape at `registry.rs:141`.
- Querying the resolved single-platform manifest digest matches the "which SBOM applies to what I'm actually pulling" mental model an operator has. The multi-arch index digest is an internal implementation detail; consumers care about the platform-specific SBOM.
- The endpoint MAY support filtering via `?artifactType=<type>` (per spec §Referrers Filtering), but m186 uses the un-filtered endpoint + client-side media-type filter for simplicity. Server-side filtering is an optimization deferred to a follow-up.

**Alternatives considered**:
- **Custom Referrers response type** — rejected. `oci_spec::image::ImageIndex` already models the response verbatim.
- **Query the index digest instead of the resolved manifest digest** — rejected. Would fetch a superset of referrers across all platforms, including ones the operator will never scan; wastes bandwidth and complicates media-type filtering with per-platform context.

---

### Decision 2 — Media-type filter + priority ordering (FR-004)

**Decision**: The filter accepts three initial media types: `application/spdx+json`, `application/vnd.cyclonedx+json`, `application/vnd.cyclonedx+xml`. When multiple matching descriptors are present, the priority order is:
1. **Explicit format match** — the descriptor whose media type matches the FIRST `--format` value the operator requested. (If operator requested `--format cyclonedx-json`, prefer the `application/vnd.cyclonedx+json` descriptor.)
2. **CDX-first fallback** — if no explicit format match, prefer any `application/vnd.cyclonedx+json` descriptor over `application/spdx+json` over `application/vnd.cyclonedx+xml`.
3. **First-descriptor tiebreaker** — if multiple descriptors match the same tier, pick the FIRST in the ImageIndex `manifests[]` array (deterministic ordering per Distribution Spec §Referrers Response).

**Rationale**:
- Format-match preference minimizes the "referrer format doesn't match my requested output format" edge case surface — which is the most common source of operator confusion.
- CDX-first fallback reflects mikebom's own emission bias (CDX is mikebom's default first-class format). The rank order is documented in the operator-facing help text.
- Deterministic tiebreaker prevents flapping behavior across scans of the same image with slightly-different registry state.

**Alternatives considered**:
- **Latest-created descriptor wins** — rejected. `created` timestamps in OCI descriptors are optional and unreliable; operators can't verify freshness from mikebom logs.
- **Emit ALL matching referrers** — deferred to a follow-up (spec.md §Deferred). MVP emits at most one.

---

### Decision 3 — Opaque-bytes emission (FR-006)

**Decision**: The fetched referrer descriptor's blob body is written to the operator's `--output` path BYTE-IDENTICALLY. No re-parsing, no re-encoding, no format normalization, no PURL rewriting, no timestamp mutation. The `mikebom:sbom-source = "referrer"` and `mikebom:sbom-source-descriptor-digest` provenance markers appear in mikebom's OWN scan-run log stream (INFO level), NOT in the emitted content.

**Rationale**:
- Preserves any upstream signer's byte-identity contract. A Cosign-signed SBOM or an in-toto-DSSE-wrapped SBOM only round-trips if the payload bytes are byte-identical to what was signed.
- Eliminates a complex source-of-bugs surface (schema validation, license normalization, PURL canonicalization on referrer bytes). mikebom's scan pipeline does all those things; a fetched referrer already went through them upstream.
- The provenance markers landing in mikebom's log stream (not in the SBOM content) means operators can audit the source via `journalctl -u mikebom | grep sbom-source` or equivalent, WITHOUT needing to mutate the emitted file.

**Alternatives considered**:
- **Merge referrer + scan output** — rejected. Doubles the emission and creates format collisions.
- **Emit provenance as CDX properties in the referrer content** — rejected. Violates byte-identity per the signer's contract.
- **Emit a sidecar `<output>.mikebom.json` provenance file** — considered. Deferred to a follow-up milestone; MVP relies on log-stream provenance which most audit workflows consume already.

---

### Decision 4 — Size-cap enforcement (`MIKEBOM_REFERRER_MAX_BYTES`, default 100 MiB)

**Decision**: mikebom enforces a per-referrer content size cap read from the descriptor's `size` field BEFORE issuing the blob fetch. When the cap is exceeded, mikebom skips the referrer with a WARN log naming the descriptor digest + declared size + configured cap. Follow-through behavior matches FR-008 (silent fall-through under `either`, error under `referrer`).

**Rationale**:
- The `size` field is authoritative per OCI Descriptor spec (§Descriptors); a malicious or misconfigured registry MAY declare an oversize `size` to DoS mikebom scanners. Enforcing the cap AT DESCRIPTOR-LEVEL (before blob fetch) prevents wasted bandwidth on malicious or misconfigured artifacts.
- 100 MiB default matches the m036 layer-cache upper-bound convention. Typical SBOMs are 100 KiB to 5 MiB; a 100 MiB threshold has ~20x headroom for realistic edge cases.
- Environment-variable override (`MIKEBOM_REFERRER_MAX_BYTES`) matches the m036 `MIKEBOM_OCI_CACHE_SIZE` pattern for operators who need edge-case flexibility (e.g., a mega-image with a huge SPDX 3 rollup).
- Post-fetch verification (via `verify_sha256`) still catches undersized-but-corrupt payloads. The cap is a first-line defense; the SHA-256 verify is the second.

**Alternatives considered**:
- **No cap** — rejected. Malicious-registry DoS attack surface.
- **Runtime cap via CLI flag** — rejected in favor of env var. CLI-flag proliferation on `sbom scan` is already high; env var covers the rare edge case without cluttering `--help`.
- **Streaming size check during fetch** — deferred. Descriptor-level check catches ≥95% of the surface; streaming adds complexity.

---

### Decision 5 — Integration test infrastructure (m182 wiremock reuse)

**Decision**: All m186 integration tests use `wiremock = "0.6"` (workspace dev-dep since m055; extended for m182). Tests spin up a `MockServer`, mount handlers for:
- `GET /v2/<repo>/manifests/<tag-or-digest>` → returns a minimal OCI image manifest (with a stable digest so referrers can reference it).
- `GET /v2/<repo>/referrers/<manifest-digest>` → returns an ImageIndex with 0/1/many descriptors of varying media types.
- `GET /v2/<repo>/blobs/<referrer-descriptor-digest>` → returns SBOM bytes (a small SPDX 2.3 or CDX 1.6 JSON body).

The `wiremock::MockServer` binds to `127.0.0.1:<ephemeral>` over PLAIN HTTP; mikebom is invoked via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` with `--insecure-registry <hostport>` (the m182 flag) to accept the plain-HTTP endpoint. This is the exact same pattern m182's `oci_pull_plain_http.rs` uses; no new test infrastructure is needed.

**Rationale**:
- Reuses m182's proven end-to-end integration-test scaffold (already ships 3 tests passing per the m182 US1 delivery). Zero learning curve for implementers.
- `wiremock` is already a workspace dev-dep; zero new dev-deps for m186.
- `--insecure-registry` unlocks HTTP-based testing without spinning up a TLS stack.

**Alternatives considered**:
- **Real Docker Hub / GHCR integration tests** — rejected. Requires network + authenticated pulls; brittle for CI.
- **HTTP mocking without wiremock** — considered `mockito` (another workspace transitive), but wiremock's `MockServer::start().await` + `Mock::given(method).and(path).mount(&server)` shape is what m182 already uses.

---

## Bug Discovery

None so far. The m186 delivery is a pure additive feature: existing behavior is preserved byte-identically when `--sbom-source scan` (the default) is active.
