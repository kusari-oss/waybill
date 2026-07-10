# Feature Specification: OCI registry TLS + transport flexibility

**Feature Branch**: `182-oci-registry-tls-flex`
**Created**: 2026-07-10
**Status**: Draft
**Input**: User description: "m182 — unblocks the Harbor plugin work. mikebom's OCI pull path today can only reach public-CA HTTPS registries (Docker Hub, GHCR, gcr.io, ECR). Three gaps prevent it from reaching private/dev registries: (a) `https://` is hardcoded, so plain-HTTP registries like Harbor's own docker-compose devenv (`http://core:8080`) can't be pulled; (b) the reqwest client trusts only webpki roots — private company CAs are rejected at TLS handshake; (c) there's no `--insecure-tls-skip-verify` escape hatch for CI/dev instances with self-signed certs. Ship three CLI flags mirroring the podman/skopeo/docker precedent: `--insecure-registry <host>` (repeatable, per-host — enables http:// for the named registry), `--registry-ca-cert <path>` (repeatable — adds a PEM cert to the trust store on top of webpki), `--insecure-tls-skip-verify` (boolean — disables cert verification globally). Bearer auth already works via the existing `WWW-Authenticate` handling — no changes there. The user asked for this because a bot report investigating Harbor plugin viability flagged these three specific transport gaps."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Scan an image from a plain-HTTP registry (Harbor devenv target) (Priority: P1)

An operator running Harbor's docker-compose devenv (or any dev-mode OCI registry that exposes its API on plain HTTP) needs to scan an image hosted there. Today `mikebom sbom scan --image http://core:8080/library/myapp:1.0` fails at the TLS handshake because mikebom rewrites the URL to `https://core:8080/…` and the port has no TLS listener. The operator wants to opt into plain-HTTP for that specific registry via a CLI flag — matching podman's `[[registry]] insecure = true` and Docker's `insecure-registries` daemon config precedent — and have the scan succeed with the exact same downstream SBOM shape as an HTTPS scan of the same content.

**Why this priority**: This is the specific blocker the Harbor plugin work surfaced. Without US1, mikebom cannot integrate with Harbor's own reference devenv, and Kusari's Harbor-plugin story dies at the first developer-experience step. Same story extends to any organization running a local OCI registry (dev clusters, air-gapped mirrors, integration-test rigs).

**Independent Test**: Stand up a plain-HTTP OCI registry (via `wiremock` in the test suite; via `docker run -p 5000:5000 registry:2` for manual verification), push a small OCI image to it, run `mikebom sbom scan --image <ref> --insecure-registry <host:port>`, and confirm mikebom pulls the manifest + blobs over HTTP and produces the expected SBOM.

**Acceptance Scenarios**:

1. **Given** a plain-HTTP OCI registry at `http://<host>:<port>`, **When** the operator runs `mikebom sbom scan --image <host>:<port>/library/img:tag --insecure-registry <host>:<port>`, **Then** mikebom MUST issue its manifest/blob GETs against `http://<host>:<port>/v2/…` and succeed with the same downstream SBOM shape as an HTTPS pull of equivalent content.
2. **Given** the same setup but WITHOUT the `--insecure-registry` flag, **When** the operator runs `mikebom sbom scan --image <host>:<port>/library/img:tag`, **Then** mikebom MUST fail with an actionable error message that names `--insecure-registry <host>:<port>` as the specific fix; it MUST NOT silently succeed over HTTPS-that-happens-to-be-listening.
3. **Given** `--insecure-registry a.example` is set AND the image reference points at `b.example`, **When** the scan runs, **Then** mikebom MUST use HTTPS for `b.example` (host-scoped flag — an insecure declaration for A does NOT downgrade B).
4. **Given** `--insecure-registry` is repeated multiple times, **When** any of the named hosts is contacted during the scan, **Then** mikebom MUST honor each declaration independently.

---

### User Story 2 — Scan an image from a private-CA registry (Priority: P1)

An operator running a production Harbor (or any private OCI registry) whose TLS certificate is signed by a company-internal CA needs to scan images there. Today mikebom's TLS trust store contains only webpki roots (`webpki-roots` crate embedded via `rustls-tls`), so the private CA's signature chain fails validation and the TLS handshake dies before any Bearer challenge fires. The operator wants to point mikebom at their internal CA's PEM file(s) — matching skopeo's `--cert=<file>` and podman's `--registry-cert-dir` precedent — and have the scan succeed. Multiple CA certs must be composable (a Harbor deployment behind a corporate reverse proxy may need both the internal CA and an intermediate cross-signed root).

**Why this priority**: The private-CA case is the common Harbor prod deployment shape. Without US2, Kusari's own dogfooding of the Harbor plugin against internal deployments fails.

**Independent Test**: Stand up an HTTPS OCI registry with a test CA (generated in the test setup via `rcgen` or shell-out to `openssl`), point mikebom at the CA PEM via `--registry-ca-cert /path/to/ca.pem`, run the scan against a served image, and confirm success.

**Acceptance Scenarios**:

1. **Given** a private-CA-signed HTTPS OCI registry, **When** the operator runs `mikebom sbom scan --image <host>/repo:tag --registry-ca-cert <path-to-pem>`, **Then** the TLS handshake MUST succeed using the additional PEM as an accepted root, and the scan MUST proceed through Bearer auth (unchanged) to produce the expected SBOM.
2. **Given** the same registry but WITHOUT `--registry-ca-cert`, **When** the scan runs, **Then** mikebom MUST fail with a message that names the flag as the fix and identifies the specific TLS validation failure (chain-of-trust break at root).
3. **Given** `--registry-ca-cert` is repeated with multiple PEM files, **When** the scan runs, **Then** mikebom MUST add every certificate to the trust store additively (webpki + all provided PEMs — nothing is removed).
4. **Given** `--registry-ca-cert /path/to/nonexistent.pem` OR an unparseable PEM, **When** the scan starts, **Then** mikebom MUST fail with an actionable error message (file-not-found OR invalid-PEM diagnostic) BEFORE the network call — no cryptic runtime crash.

---

### User Story 3 — Skip TLS verification for CI/dev instances (Priority: P1)

An operator running a CI pipeline against a mikebom-scan-able instance whose HTTPS cert is self-signed (or expired, or hostname-mismatched — anything the standard trust chain rejects) needs an escape hatch. `--registry-ca-cert` doesn't help when the operator doesn't have (or doesn't want to fetch) the specific CA — sometimes the answer is just "trust anything for this scan". The operator wants a `--insecure-tls-skip-verify` flag matching podman's `--tls-verify=false` and skopeo's `--src-tls-verify=false` precedent, with the same widely-understood security tradeoff.

**Why this priority**: The CI/dev case is the third of three specific gaps the bot flagged. mikebom shipping without ANY skip-verify escape hatch forces operators to either fetch/provision the correct CA or abandon the scan — that's a worse user experience than every peer tool (docker, podman, skopeo, crane).

**Independent Test**: Stand up an HTTPS OCI registry with a bogus self-signed cert (bad hostname OR expired), point mikebom at it with `--insecure-tls-skip-verify`, and confirm the scan succeeds. WITHOUT the flag, confirm it fails.

**Acceptance Scenarios**:

1. **Given** an HTTPS OCI registry whose cert would fail standard validation (self-signed, expired, hostname-mismatched), **When** the operator runs `mikebom sbom scan --image <host>/repo:tag --insecure-tls-skip-verify`, **Then** the scan MUST succeed and produce the expected SBOM.
2. **Given** the same registry WITHOUT the flag, **When** the scan runs, **Then** mikebom MUST fail with a message identifying the TLS validation failure AND listing both `--registry-ca-cert` and `--insecure-tls-skip-verify` as candidate fixes (in that order — the safer fix first).
3. **Given** `--insecure-tls-skip-verify` is enabled during a scan, **When** mikebom starts the pull, **Then** it MUST emit a WARN-level structured log naming the skip-verify state and the affected image ref, so operators auditing scan logs later can identify unverified pulls.

---

### User Story 4 — Multi-flag composition + precedence (Priority: P2)

An operator running mikebom across a heterogeneous set of registries within one scan (a shade-jar scan that references public GHCR + a private Harbor + a plain-HTTP local mirror, for instance) needs all three flags to compose sensibly. The plain-HTTP `--insecure-registry a.example:5000` MUST NOT weaken the trust for the private `b.example` HTTPS pull; `--registry-ca-cert` MUST NOT interfere with the public `ghcr.io` pull whose cert already validates against webpki; `--insecure-tls-skip-verify` when set MUST override the CA-cert validation on ALL HTTPS registries in the scan (it's the loudest hammer).

**Why this priority**: Regression pin against the "one flag surprised me by affecting a different registry" class of bug. Elevated to P2 (not P3) because a subtle cross-flag bug here would be operationally scary.

**Independent Test**: A single mikebom invocation touches 3 registries (public HTTPS, private-CA HTTPS, plain-HTTP local) with all three flags combined; each registry pulls correctly.

**Acceptance Scenarios**:

1. **Given** all three flags set with values pointing at distinct hosts, **When** the scan pulls from each host, **Then** each host's transport respects its OWN configuration (host A's insecure flag does NOT downgrade host B).
2. **Given** `--insecure-tls-skip-verify` AND `--registry-ca-cert` both set, **When** an HTTPS host is contacted, **Then** the skip-verify wins (skip-verify is a superset of "trust the additional CA").
3. **Given** `--insecure-registry a` AND `--insecure-tls-skip-verify`, **When** host A is contacted, **Then** the plain-HTTP path applies (skip-verify is moot for HTTP — no TLS handshake to skip).

---

### Edge Cases

- **`--insecure-registry <host>` with no port**: matches any port on that host. Explicit-port form `<host>:<port>` matches only that port. Matches podman's registries.conf `[[registry]] location =` behavior.
- **`--insecure-registry` with a registry name that maps to a different actual endpoint** (e.g., `docker.io` → `registry-1.docker.io`): the flag matches on the USER-FACING name (what appears in the `--image` ref), not the resolved endpoint. This preserves the mental model that operators declare "the registry I typed at the CLI is insecure".
- **`--registry-ca-cert` PEM containing multiple certificates**: mikebom MUST add ALL certificates in the PEM (a bundle) to the trust store, not just the first.
- **`--registry-ca-cert` used against a public-CA registry that would validate without the extra CA**: silently permitted — the additional CA is additive, not a replacement.
- **`--insecure-tls-skip-verify` + a registry that would fail Bearer auth**: mikebom MUST still surface the Bearer failure (skip-verify only bypasses TLS chain validation; it does NOT bypass application-layer auth).
- **Existing `--registry-credentials-dir` flag coexistence**: unchanged — credentials resolution and TLS configuration are orthogonal layers. All three m182 flags apply to how mikebom REACHES the registry; `--registry-credentials-dir` applies to HOW mikebom AUTHENTICATES once it's connected.
- **Public HTTPS Docker Hub pull with NONE of the m182 flags set**: MUST produce byte-identical SBOM output pre-vs-post m182 (regression guard — no operator running a public-registry scan should see any change in behavior).
- **URL scheme in `--image http://…`**: does NOT implicitly enable insecure. The scheme is a hint; the explicit flag is required. Matches Docker's behavior (podman auto-downgrades; docker requires the flag; mikebom follows docker for surprise-free operator experience). Documented in FR-013.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST accept a new CLI flag `--insecure-registry <host[:port]>` on the `sbom scan` subcommand, repeatable to declare multiple hosts. When a manifest or blob URL's target host matches a declared value, mikebom MUST issue the request over `http://` instead of `https://`.
- **FR-002**: mikebom MUST accept a new CLI flag `--registry-ca-cert <path>` on the `sbom scan` subcommand, repeatable to load multiple PEM files. Each file's certificate(s) MUST be added to the HTTP client's trust store additively; the existing webpki root set MUST remain in place.
- **FR-003**: mikebom MUST accept a new CLI flag `--insecure-tls-skip-verify` (boolean) on the `sbom scan` subcommand. When set, the HTTP client MUST accept any TLS certificate presented by an HTTPS OCI endpoint without chain validation, hostname verification, or expiry checks.
- **FR-004**: When `--insecure-registry` matches on host-only (no port) form, mikebom MUST match any port on that host in the URL. Explicit `host:port` form MUST match only that exact host:port.
- **FR-005**: `--insecure-registry` matching MUST operate on the user-facing registry name (the string the operator typed in `--image` or that the image reference parses to as the `registry` component), NOT the resolved endpoint (e.g., not `registry-1.docker.io` for `docker.io`). Consistent with FR-004's mental model of "the registry I typed at the CLI is what I'm declaring insecure".
- **FR-006**: `--registry-ca-cert` MUST accept PEM-format files. Files may contain MULTIPLE PEM certificates concatenated (a "bundle"); ALL certificates in each file MUST be added to the trust store. Non-existent files MUST fail with a file-not-found error before any network call. Unparseable PEM content MUST fail with an actionable diagnostic naming the file.
- **FR-007**: `--insecure-tls-skip-verify` MUST emit a WARN-level structured log at scan startup naming the flag-state and the affected image ref, per Constitution Principle X (Transparency).
- **FR-008**: When `--insecure-tls-skip-verify` AND `--registry-ca-cert` are BOTH set, the skip-verify behavior wins for HTTPS pulls (skip-verify is a superset of "trust the additional CA"). The `--registry-ca-cert` flag is NOT rejected in this combination — it's merely overridden.
- **FR-009**: When `--insecure-registry` matches AND `--insecure-tls-skip-verify` is also set, the plain-HTTP path applies (skip-verify is moot for HTTP — no TLS handshake to bypass).
- **FR-010**: When a scan touches multiple registries in one invocation (shade-jar closure, --image referencing a multi-arch manifest that points at another registry, etc.), each host's transport MUST be resolved INDEPENDENTLY per the m182 flags. Host A's insecure declaration MUST NOT downgrade Host B.
- **FR-011**: All three m182 flags MUST coexist with the existing `--registry-credentials-dir` and `--image-src`/`--image` flags without behavior conflicts. Credentials resolution and TLS configuration are orthogonal layers.
- **FR-012**: For scans that pass NONE of the three new flags AND target a public-CA HTTPS registry, mikebom's emitted SBOM MUST be byte-identical to the pre-m182 baseline (regression guard against unintended default-mode drift).
- **FR-013**: A URL scheme prefix (`http://` OR `https://`) in `--image <ref>` MUST NOT implicitly enable insecure or skip-verify behavior. The explicit m182 flag MUST be required for these behaviors. Documented rationale: docker's mental model wins over podman's for surprise-free operator experience.
- **FR-014**: mikebom MUST produce actionable error messages when TLS or transport failures occur:
    - Plain-HTTP registry contacted without `--insecure-registry` → error names the flag as the fix
    - Private-CA registry contacted without `--registry-ca-cert` → error names both `--registry-ca-cert` and `--insecure-tls-skip-verify` as candidate fixes (in that order — the safer one first)
    - Non-existent CA cert path → file-not-found diagnostic
    - Unparseable CA cert PEM → invalid-PEM diagnostic naming the file

### Key Entities

- **Registry Transport Configuration** (internal, per-scan): the composition of all m182 flags plus their per-host expansion into per-URL transport decisions. Passed to the OCI HTTP client at construction time. Immutable for the duration of a scan.
- **Insecure Registry Match Table** (internal): the parsed `--insecure-registry` flag values expanded into a match function `(host, port) → bool`. Applied per-request.
- **Custom CA Bundle** (internal): the union of all `--registry-ca-cert` files' PEM contents, parsed into `rustls::Certificate` instances (or equivalent) and passed to the HTTP client builder's `.add_root_certificate()` for each entry.
- **Skip-Verify Sentinel** (internal): a boolean threaded into the HTTP client builder's `.danger_accept_invalid_certs()` call.
- **Actionable Error Messages** (user-facing): three distinct error paths (plain-HTTP-no-flag, private-CA-no-flag, PEM-load-failure) each with dedicated diagnostic text.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator can scan an image hosted on Harbor's docker-compose devenv (`http://core:8080/library/foo:latest`) end-to-end using `mikebom sbom scan --image core:8080/library/foo:latest --insecure-registry core:8080` — the scan completes and produces a valid SBOM.
- **SC-002**: An operator can scan an image hosted on a private-CA HTTPS registry using `mikebom sbom scan --image <host>/repo:tag --registry-ca-cert /path/to/ca.pem` — the scan completes and produces a valid SBOM.
- **SC-003**: An operator can scan an image hosted on an HTTPS registry with a self-signed cert using `mikebom sbom scan --image <host>/repo:tag --insecure-tls-skip-verify` — the scan completes AND emits a WARN log naming the skip-verify state.
- **SC-004**: A scan with NONE of the three m182 flags set, targeting a public-CA HTTPS registry (Docker Hub, GHCR, gcr.io) produces byte-identical CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output pre-vs-post m182 for the same input.
- **SC-005**: When the operator forgets a required m182 flag, the resulting error message ALWAYS names the specific flag(s) that would fix the scenario. Zero cases of "TLS failure" or "connection refused" with no fix guidance.
- **SC-006**: The three flags compose sensibly per FR-008 + FR-009 + FR-010 — a scan touching three distinct registries with distinct transport requirements in one invocation completes without cross-contamination.
- **SC-007**: The Harbor plugin devenv target — Harbor's own `docker-compose up` produced local registry at `http://core:8080` — is scannable by mikebom in a single command invocation.
- **SC-008**: The dev-tool user experience for the three failure modes (plain-HTTP-no-flag, private-CA-no-flag, self-signed-no-flag) matches or beats the equivalent podman/skopeo error-message clarity when tested against the same registries — the failure includes both the root cause AND the specific fix.

## Assumptions

- The three CLI flags (`--insecure-registry`, `--registry-ca-cert`, `--insecure-tls-skip-verify`) are per-invocation only. There is NO persistent config file (no `~/.mikebom/registries.toml` in m182). This matches mikebom's existing CLI-only configuration posture — every mikebom flag today is per-invocation. A follow-up milestone MAY add a config file if operator feedback demands it.
- CLI flag naming follows the podman/skopeo/docker convention family. `--insecure-registry` matches Docker's `insecure-registries` daemon config key. `--registry-ca-cert` matches podman's per-registry cert dir pattern (simplified to a file — mikebom is a one-shot CLI, not a daemon). `--insecure-tls-skip-verify` mirrors podman's `--tls-verify=false` inverted-boolean form.
- The three flags apply globally to a scan invocation — they don't scope to a specific `--image` when multiple images are involved. This is a simplifying assumption; if the Harbor team's tests reveal multi-image scans need per-image scoping, the flags can be extended to per-image variants in a follow-up milestone without breaking m182's API.
- PEM is the ONLY supported CA cert format. No DER, no PKCS#12. This matches skopeo's default. Adding DER support is trivial if requested.
- Bearer + Basic auth are already implemented — no auth changes in m182. See `mikebom-cli/src/scan_fs/oci_pull/registry.rs::fetch_bearer_token` (line 267) and `AuthChallenge::Bearer` handling at lines 199-208.
- The existing `reqwest::Client` construction site at `RegistryClient::new` (line 79 of registry.rs) is the SINGLE integration point for all three flags. No new HTTP client is introduced; the existing one gains config.
- `webpki-roots` (the workspace-embedded trust store) is unchanged — additional CA certs are additive, not a replacement.
- The Harbor team may report a fourth gap during testing (mTLS client auth, gzip-encoded blobs, some Harbor-specific quirk). m182's plumbing is designed to accept a fourth flag cheaply if that happens — but the fourth flag is NOT included in m182.
- Testing uses a mix of `wiremock` (already a dev-dep per m055) for pure-Rust unit-level fixtures plus `rcgen` (may need to be added as a dev-dep — verify at planning time) for generating throwaway test CAs. If `rcgen` is contested at review, a shell-out to `openssl req` inside test setup is a fallback.

## Constitution Alignment

**Principle III (Fail Closed)**: The three flags are opt-in escape hatches. mikebom's default behavior (no flags set) is UNCHANGED and remains fail-closed against unknown/untrusted transports. The operator has to explicitly ask for degraded security posture. Each opt-in requires the operator to type its exact name — no config-file surprises.

**Principle X (Transparency)**: FR-007 mandates a WARN-level structured log when `--insecure-tls-skip-verify` is active. FR-014 mandates actionable error messages for the three failure modes. The operator can always audit AFTER a scan to determine which flags were in effect.

**Principle IV (Type-Driven Correctness)**: `--insecure-registry` values are parsed into a typed `HostMatcher` struct (host-only vs host:port); `--registry-ca-cert` paths are validated at parse time; `--insecure-tls-skip-verify` is a `bool` on a typed CLI struct. No raw string threading through the OCI pull code path.

**Principle V (Specification Compliance / Native-first)**: N/A — no SBOM emission changes in m182. The CLI-flag additions do not introduce any `mikebom:*` fields.

**No new Cargo dependencies expected** — `rustls`/`reqwest 0.12` already support both `.add_root_certificate()` and `.danger_accept_invalid_certs()`. If `rcgen` is used for test-cert generation, it's dev-only and doesn't affect the shipped binary.

## Deferred to Future Milestones

- **mTLS client-cert authentication** — mikebom presenting a client cert to the registry. The Harbor team's test may or may not reveal this as needed. Deferred by design.
- **Persistent config file** (`~/.mikebom/registries.toml` or similar) — deferred until operator feedback demands it.
- **Per-image flag scoping** (attach flags to specific `--image` refs when multiple images are scanned) — deferred until multi-image scan use cases surface.
- **DER cert format** support alongside PEM — deferred; PEM covers all documented Harbor/prod-CA distribution mechanisms.
