# Feature Specification: Scan Local Podman Images

**Feature Branch**: `206-podman-source`
**Created**: 2026-07-17
**Status**: Draft
**Input**: Issue #440 — Add podman daemon / rootless source support.

## Background

Podman is the dominant Docker alternative on RHEL, Fedora, and modern rootless-first CI. Container images built or pulled with `podman` live in a per-user storage tree (`~/.local/share/containers/storage/` for rootless; `/var/lib/containers/storage/` for rootful) that the current mikebom docker-daemon source does not touch. Operators using podman as their local container tool today get **zero inventory** for their local images — they either have to switch to a docker-installed machine, push the image to a registry and re-pull via `--image <registry-ref>`, or hand-invoke `podman save` and feed the tarball to mikebom.

m206 closes that gap: mikebom scans local podman images directly, matching the operator ergonomic of the existing docker-daemon source.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Rootless podman image scan (Priority: P1)

An operator on a Linux workstation or CI runner uses `podman` to build or pull container images. They invoke mikebom against a locally-stored podman image by tag or ID; mikebom reads the image out of `~/.local/share/containers/storage/`, assembles a rootfs, and produces an SBOM using the same emission pipeline as every other image source.

**Why this priority**: Rootless podman is the majority use case (default install on Fedora 34+, common in air-gapped CI). Without this path, mikebom is functionally unavailable to podman-first operators. P1 because it unblocks an entire class of users who currently have no ergonomic option.

**Independent Test**: On a Linux machine with `podman` installed, `podman pull alpine:latest`. Run `mikebom sbom scan --image alpine:latest --format cyclonedx-json --output out.cdx.json` (or the equivalent explicit `--image-src podman` invocation). Assert (a) scan exits 0, (b) `components[]` contains at least the alpine base packages (`apk` reader ran against the extracted rootfs), (c) `metadata.component.name` reflects `alpine`.

**Acceptance Scenarios**:

1. **Given** a rootless podman image cached at `~/.local/share/containers/storage/`, **When** the operator runs `mikebom sbom scan --image <name>:<tag>`, **Then** the emitted SBOM contains the image's rootfs components (same detection quality as `docker pull` + mikebom scan of the same image).
2. **Given** the same image is available BOTH in podman (locally) AND at a registry (remote), **When** the operator's `--image-src` preference lists podman first, **Then** mikebom reads from podman without a network round-trip.
3. **Given** an image ID (SHA-256 digest) instead of a tag, **When** the operator passes `--image sha256:<digest>`, **Then** mikebom resolves it from podman's storage and scans it identically.

---

### User Story 2 - Rootful podman image scan (Priority: P2)

An operator running mikebom under `sudo` (or as root) targets a rootful podman image stored at `/var/lib/containers/storage/`. Same scan-and-emit flow as US1.

**Why this priority**: Rootful podman is less common on end-user machines but standard in some CI configurations and in production-image build farms. P2 because it's a straightforward extension of the US1 code path once the storage-layout parser exists; the shape of the parse + assemble pipeline is identical, only the base path differs.

**Independent Test**: `sudo podman pull alpine:latest`. Run `sudo mikebom sbom scan --image alpine:latest`. Assert the same output as US1 (byte-identical SBOM apart from `metadata.timestamp` if the two scans straddle a second boundary).

**Acceptance Scenarios**:

1. **Given** a rootful podman image cached at `/var/lib/containers/storage/`, **When** the operator (with read access to that directory) runs mikebom, **Then** the emitted SBOM matches the US1 output shape.
2. **Given** the operator lacks read access to `/var/lib/containers/storage/`, **When** they run mikebom without sudo, **Then** the scan exits non-zero with an actionable error naming the permission problem and suggesting either `sudo` or `--image-src remote` fallback.

---

### User Story 3 - Auto-detection between docker + podman sources (Priority: P2)

An operator on a machine with BOTH docker and podman installed runs `mikebom sbom scan --image <name>:<tag>` without specifying `--image-src`. mikebom probes each source in a documented default order and uses the first one that has the image cached.

**Why this priority**: End-users routinely have both container tools installed for compatibility testing (docker for legacy tooling, podman for the actual daily driver). Requiring them to remember which source has the image is friction. P2 because US1 delivers the podman-scan core value; auto-detection is UX polish on top.

**Independent Test**: Install both docker and podman. `podman pull alpine:latest` (docker cache empty). Run `mikebom sbom scan --image alpine:latest` with no `--image-src` flag. Assert (a) mikebom does NOT report a "not found in docker" error, (b) the podman-cached image is found and scanned. Repeat with the roles reversed (docker has alpine, podman doesn't).

**Acceptance Scenarios**:

1. **Given** an operator with docker + podman installed AND the target image cached in podman only, **When** they run mikebom with the default `--image-src` list, **Then** mikebom scans the podman-cached image without erroring on the docker-absent case.
2. **Given** the image is cached in BOTH docker and podman, **When** the default order preference lists docker first (industry-standard ordering), **Then** mikebom uses docker to preserve byte-identity with pre-m206 behavior.
3. **Given** the operator explicitly sets `--image-src podman`, **When** they run mikebom, **Then** mikebom uses podman exclusively even if docker has the image; the source-selection preference is respected verbatim per FR-005.

---

### Edge Cases

- **Image tag ambiguity**: podman allows `<repo>:<tag>` and `<repo>@sha256:<digest>` and `<image-id-short>` (12-char prefix). mikebom MUST accept all three forms per parity with existing docker-source behavior.
- **Storage-driver diversity**: podman supports `overlay` (default), `vfs`, and `btrfs` storage drivers. m206 supports `overlay` (the default on 95%+ of installs) as MVP; `vfs` and `btrfs` are stretch — non-fatal skip with WARN naming the driver.
- **Multi-arch images**: podman may cache multiple architectures of the same tag (e.g., `alpine:latest` for both `linux/amd64` and `linux/arm64`). mikebom's existing behavior for docker-source is to scan the image matching the host arch OR error if ambiguous; podman-source matches (FR-011).
- **Corrupted / partial storage**: an interrupted `podman pull` may leave `overlay-images/<id>/` half-populated. mikebom MUST detect and skip cleanly (WARN, exit non-zero on the specific target, do not corrupt the rest of the scan).
- **Storage moved via `containers.conf` `graphroot` override**: podman honors `containers.conf` to relocate the storage root. mikebom MUST honor the same setting when reachable (read `~/.config/containers/storage.conf` or `/etc/containers/storage.conf`) OR document the override via a `--podman-storage-root <path>` escape hatch. Reasonable-default MVP: read the config; escape hatch is Phase-2 if operators surface a need.
- **Non-Linux hosts**: podman's storage layout is Linux-specific (relies on OverlayFS or similar kernel features). On macOS podman runs inside a Linux VM (`podman machine`), and its storage lives inside that VM — not directly accessible from mikebom running on the host. Non-Linux hosts MUST error clearly on `--image-src podman` naming the "podman machine" limitation. Operators on macOS should use `podman save > out.tar` + `mikebom scan --image out.tar` per the existing tarball-source path.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST support scanning a local podman image referenced by name (`<repo>:<tag>`) via the existing `--image <ref>` CLI flag (unified with docker + remote sources).
- **FR-002**: The podman source MUST support both rootless (`~/.local/share/containers/storage/`) and rootful (`/var/lib/containers/storage/`) storage roots, selecting rootless by default when running as a non-root user.
- **FR-003**: The podman source MUST accept image references in the same forms as the docker source: `<repo>:<tag>`, `<repo>@sha256:<digest>`, and `<image-id>` (full or 12-char prefix).
- **FR-004**: An `--image-src` flag value `podman` MUST be accepted alongside the existing `docker` and `remote` values. Ordered comma-separated lists (e.g., `--image-src podman,docker,remote`) MUST be honored per m031's existing preference-ordering semantics.
- **FR-005**: When multiple sources have the same image cached, `--image-src` ordering MUST be respected verbatim — the operator's stated preference is not overridden by mikebom heuristics.
- **FR-006**: The default `--image-src` ordering when podman is available MUST include podman as one of the fallbacks, but MUST NOT break byte-identity for existing docker-source scans on machines without podman. Concretely: the default list expands from `docker,remote` (pre-m206) to `docker,podman,remote` (post-m206). Docker-first preserves backward compat; podman is inserted before remote so local podman images are preferred over network fetches.
- **FR-007**: When podman is unavailable (storage directory missing / unreadable / storage driver unsupported), mikebom MUST fall back to the next source in the `--image-src` list without erroring the whole scan. If no source succeeds, mikebom MUST exit non-zero with an actionable error naming which sources were tried + why each failed.
- **FR-008**: The scan output of a podman-source scan MUST be structurally equivalent to a docker-source scan of the same image content — same component set, same PURLs, same evidence shape. Differences allowed only for source-identity fields (e.g., a `mikebom:image-source = "podman"` marker vs `"docker"` in analogous slots).
- **FR-009**: The podman source MUST NOT depend on the podman daemon or REST API. Filesystem-read only. (Rationale: operators using rootless podman may not have the socket enabled; mikebom should work in every legit podman config.) Optional API-fallback is out of scope for m206 and is tracked as follow-up.
- **FR-010**: The podman source MUST honor `containers.conf`/`storage.conf`'s `graphroot` override when a config file is present at the standard paths (`$HOME/.config/containers/storage.conf` for rootless, `/etc/containers/storage.conf` for rootful). When absent, use the compiled-in defaults per FR-002.
- **FR-011**: For multi-arch images, mikebom MUST pick the image variant matching the host architecture (`x86_64`, `aarch64`, etc.) by default. If no host-arch match is present, mikebom MUST exit non-zero naming the available architectures + the host arch — same UX as the existing docker-source multi-arch behavior.
- **FR-012**: The podman-storage read path MUST NOT execute code from the image (no chroot exec, no init-run) — the only operations are file-read + layer overlay extraction, matching the existing docker/oci_pull path's safe posture.
- **FR-013**: The scan MUST work offline (no network access) for images already cached by podman. `--offline` flag propagates unchanged.
- **FR-014**: The podman source MUST surface a document-level source-identity signal — same shape as the existing docker source. A `mikebom:image-source` metadata property (or the standards-native equivalent already used for docker-source) MUST carry the value `"podman"` when podman was the winning source. This is consumer-visible so downstream tooling can distinguish scan provenance.

### Key Entities *(include if feature involves data)*

- **Podman image reference**: the string an operator provides via `--image`. May be a tagged repo, digest-pinned repo, or bare image ID. Resolves against podman's storage-tree indexes to a concrete OCI image identity.
- **Podman storage root**: filesystem directory podman uses to store layers, image metadata, and layer indexes. Rootless (per-user) or rootful (system-wide). Configurable via `containers.conf`.
- **Extracted rootfs**: the flat filesystem tree assembled by overlaying the image's layers in order. mikebom's package-DB and file-tier readers scan this identically to how they scan any other rootfs — no source-specific downstream code.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A rootless-podman-cached `alpine:latest` scan produces the same non-zero apk package count as a docker-cached scan of the same image (deterministic across three back-to-back runs).
- **SC-002**: An operator whose only container tool is podman can run `mikebom sbom scan --image <name>:<tag>` successfully with no additional flags or configuration steps — the m206 "just works" acceptance.
- **SC-003**: When podman is uninstalled or the storage root is empty, a scan targeting a registry-only image via the default `--image-src` order still succeeds (i.e., podman's absence never breaks the fallback chain).
- **SC-004**: The emitted SBOM's `metadata.component.name` matches the image ref the operator passed (post-normalization for tag / digest / short-ID form).
- **SC-005**: `git diff --stat mikebom-cli/tests/fixtures/` post-implementation shows ZERO drift for non-podman goldens (docker-source scans + registry-source scans unchanged).
- **SC-006**: `./scripts/pre-pr.sh` wall-clock delta versus pre-m206 baseline is ≤ 10 seconds (m206 adds one new source module + one integration test group; no cross-cutting recompile).
- **SC-007**: PR description references `Closes #440`.

## Assumptions

- The target OS is Linux for the actual podman-scan feature. macOS and Windows operators get a clear error naming the "podman machine" limitation + suggested workaround (US4 edge case). Cross-platform support for `podman machine` VM introspection is explicitly out of scope for m206.
- Podman uses the `overlay` storage driver on the scanned host. `vfs` and `btrfs` are stretch targets; if the storage-driver detection returns something other than `overlay`, mikebom logs a WARN naming the driver and falls back to the next `--image-src` entry.
- The podman storage layout at `<graphroot>/overlay/`, `<graphroot>/overlay-images/`, `<graphroot>/overlay-layers/` is stable across podman v4+ and remains the target for m206. If podman v5+ changes the layout (there's no signal it will), mikebom will need a follow-up milestone to bridge.
- No new Cargo dependencies. Reuses `oci-spec` (workspace, already used by m031's registry-pull path for manifest+config parsing) + `tar` + `flate2` (workspace, layer extraction) + `sha2` + `data-encoding` (workspace, digest verification).
- The existing `docker_image::extract` + `oci_pull::tarball::assemble_docker_save_tarball` scaffold is the target composition — m206 adds a NEW module that assembles a docker-save-format tarball from podman's on-disk layers and hands it to the existing extract path. The rootfs-scan pipeline downstream of the tarball is completely unchanged.
- Operators on non-standard podman configs (`graphroot` override via `containers.conf`) get their config honored automatically per FR-010. Overriding via an explicit `--podman-storage-root <path>` flag is deferred to a follow-up milestone unless real operator need surfaces during m206 review.
- The `mikebom:image-source = "podman"` or standards-native equivalent property is a fresh emission not currently in any golden. Per Constitution Principle V, a native-first audit is required before shipping (any existing CDX/SPDX field already carrying "which local tool produced this scan"?). If no native slot exists, the `mikebom:*` property lands in the C-row catalog with the KEEP-NO-NATIVE audit rationale.
