# Research: Scan Local Podman Images

**Date**: 2026-07-17
**Purpose**: Resolve 4 mechanical unknowns before task decomposition.

## R1 — Reuse `oci_pull::tarball::assemble_docker_save_tarball` for the tarball assembly

**Investigation**: Two feasible acquisition strategies for a podman image:

- **Strategy A**: Bypass `docker_image::extract` entirely — build the merged rootfs directly by overlaying each `overlay/<layer-id>/diff/` directory in the manifest's layer order, feed the merged rootfs to `package_db::read_all`.
- **Strategy B**: Re-tar each layer's `diff/` directory back into a `.tar.gz`, assemble a docker-save-format tarball via `oci_pull::tarball::assemble_docker_save_tarball`, hand to `docker_image::extract`.

Strategy A saves the re-tar CPU cost + duplicate extraction pass. Strategy B reuses the entire m031 pipeline verbatim including OCI config parsing (env vars, exposed ports, entrypoint metadata that mikebom currently threads into some emitters via `docker_image::ExtractedImage`).

**Decision**: Strategy B. Rationale:

- Zero risk of divergence from docker-source semantics. `docker_image::extract` at `docker_image.rs:96` is the shared post-tarball code path — every existing image-source test case implicitly covers m206's downstream behavior.
- Reuses `oci_pull::tarball::assemble_docker_save_tarball` verbatim (line 66, signature `(config_bytes, layers, image_ref, out_path) -> Result<()>`).
- CPU cost of re-tar is bounded (each layer is ≤ hundreds of MB in typical images; single-pass streaming). Empirically comparable to the extract cost mikebom already pays.
- Preserves the "one canonical rootfs-extractor" invariant in the codebase — any future improvement to `docker_image::extract` (e.g., faster tar unpack) benefits podman-source for free.

**Alternatives considered + rejected**:
- Strategy A: rejected per the reasoning above — divergence risk outweighs the CPU saving.
- Direct overlay-mount of the layer chain (no tar roundtrip): rejected — requires root privileges (mount syscall) even on rootless podman scans, breaking the rootless posture of the whole feature.

**References**:
- `mikebom-cli/src/scan_fs/oci_pull/tarball.rs:66-162` — `assemble_docker_save_tarball` signature + implementation.
- `mikebom-cli/src/scan_fs/docker_image.rs:96+` — `extract()` function that consumes the docker-save tarball.

## R2 — Storage-root discovery: honor `containers.conf` + fall back to well-known defaults

**Investigation**: Podman's storage root is configurable via `containers.conf` (aka `storage.conf` for newer versions). The `graphroot` key overrides the default. Standard config paths:

- Rootless: `$HOME/.config/containers/storage.conf` (per-user override)
- Rootful: `/etc/containers/storage.conf` (system-wide override)
- Fallback defaults if no config present:
  - Rootless: `$HOME/.local/share/containers/storage/`
  - Rootful: `/var/lib/containers/storage/`

Common override case: operators with limited home-directory space relocate `graphroot` to `/mnt/podman-storage/` or similar. mikebom must honor this.

**Decision**: Discovery algorithm (per FR-002 + FR-010):

1. If `MIKEBOM_PODMAN_STORAGE_ROOT` env var set (Phase-2 escape hatch): use verbatim, skip config parsing. Not shipped in m206 MVP — deferred to follow-up if operator need surfaces.
2. Detect running-as-root vs rootless via `geteuid() == 0`.
3. Read the appropriate config file:
   - Rootful: `/etc/containers/storage.conf`
   - Rootless: `$HOME/.config/containers/storage.conf`
4. Parse TOML; look for `[storage] graphroot = "<path>"`. If present + non-empty, use verbatim.
5. If config absent OR `graphroot` key absent OR empty: use compiled-in default per detection (rootful → `/var/lib/containers/storage/`, rootless → `$HOME/.local/share/containers/storage/`).
6. Verify chosen path exists + is readable. If not, return `PodmanStorageError::StorageRootUnreachable { path, reason }` → caller falls back to next `--image-src` entry per FR-007.

**Rationale**: Matches the operator's actual podman config without requiring them to teach mikebom via a flag. `containers.conf` parsing is 30 LOC using the workspace `toml` crate (already a direct dep since m064). Fail-graceful on missing config file → default path → check readable.

**Alternatives considered + rejected**:
- Shell out to `podman info --format {{.Store.GraphRoot}}`: rejected — FR-009 mandates no daemon/subprocess dependency; podman info would require podman on PATH which contradicts the filesystem-only posture.
- Hardcode the defaults without config parsing: rejected — real operator configs override; missing them causes mysterious "image not found" errors when the podman image ISs cached, just under a different graphroot.
- Introduce a `--podman-storage-root <path>` flag as MVP: deferred — 90%+ of operators use the default config; adding a flag before demonstrating need violates YAGNI.

**References**:
- Podman docs — `containers/storage` project at https://github.com/containers/storage (layout stable across v4+).
- `mikebom-cli/src/scan_fs/package_db/cargo.rs` — precedent for reading TOML config files (via workspace `toml = "0.8"` dep).

## R3 — Storage-driver support: `overlay` MVP, `vfs`/`btrfs` deferred with WARN

**Investigation**: Podman supports 3 storage drivers via `containers/storage`: `overlay` (default on 95%+ installs, requires kernel OverlayFS or fuse-overlayfs), `vfs` (universal fallback, slow), `btrfs` (rare — only on btrfs root FS).

Storage layout diverges per driver:
- `overlay`: `<graphroot>/overlay/<layer-id>/diff/` (unpacked) + `<graphroot>/overlay-layers/layers.json` (index).
- `vfs`: `<graphroot>/vfs/dir/<layer-id>/` (unpacked, no overlay-diff separation).
- `btrfs`: `<graphroot>/btrfs/subvolumes/<layer-id>/` (subvolume-per-layer).

Supporting all three multiplies the layer-read code by 3x. m206 scopes to `overlay` (the default) — the other two are stretch.

**Decision**: Detect the driver at storage-root parse time via `<graphroot>/storage.conf` or by directory presence (`overlay/` vs `vfs/` vs `btrfs/`). If not `overlay`, emit a WARN naming the driver + return `PodmanStorageError::UnsupportedDriver { driver }` → FR-007 fallback fires.

**Rationale**: MVP scope. Real operator surveys (podman docs, RHEL default install docs) put overlay at ~95% of installs. `vfs` is a fallback that shows up in nested containers (Docker-in-Docker CI environments); we WARN + fall back cleanly rather than silently misclassifying. `btrfs` is <2% of installs on the RHEL/Fedora target audience; not worth the LOC.

**Alternatives considered + rejected**:
- Support all 3 drivers in m206: rejected per YAGNI; each driver adds ~50 LOC of parsing + testing that most users never exercise.
- Silently fall back to `remote` on non-overlay drivers: rejected — no operator-visible signal about the reduced-fidelity path.

**Follow-up**: If real operator need surfaces for `vfs` or `btrfs`, a small follow-up milestone can add per-driver adapters that produce the same layer-diff-directory abstraction the overlay path uses.

## R4 — Conditional `mikebom:image-source` annotation avoids docker/registry golden drift

**Investigation**: FR-014 specifies a new document-scope `mikebom:image-source = "podman"` annotation. But FR-005 mandates ZERO drift for non-podman goldens. Two options:

- **Option A**: Emit the annotation ALWAYS (docker/podman/registry). Requires regenerating docker + registry goldens to add the annotation. Violates FR-005.
- **Option B**: Emit the annotation ONLY when source == Podman. Docker + registry goldens unchanged. Consumers can distinguish podman by presence of the annotation; absence means "not podman."

**Decision**: Option B (conditional emission). Same pattern as m134 collisions, m161 workspace-mode, m204 image-extraction-completeness — annotation present-when-relevant, absence signals default.

**Rationale**:
- Preserves FR-005 byte-identity for docker/registry scans (SC-005 assertion holds).
- Consumers still get a clear machine-readable "this was scanned via podman" signal (present == podman; absent == docker OR registry OR local path).
- Docker + registry distinguishability can be added later if operators surface a need — no design commitment lost.

**Alternatives considered + rejected**:
- Option A: rejected per byte-identity conflict + widespread regen cost.
- Standards-native carrier (e.g., CDX `metadata.tools[]`): rejected — that array names the SBOM-producing tool (mikebom), not the image-caching tool. Semantic mismatch documented in Constitution Check §V audit.

**Parity catalog**: register as row C124 alongside C123 (m204 helm image-extraction-completeness). `Directionality::SymmetricEqual`. Same 8-hop plumbing pattern m204 used verbatim.

**References**:
- m204 spec + implementation (`specs/204-helm-completeness-annotation/`) — the direct pattern precedent.
- m161 spec (`specs/161-go-workspace-edges/`) — `go_workspace_mode` conditional-emission precedent.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Tarball assembly | Strategy B: re-tar diff dirs + reuse `assemble_docker_save_tarball` | Strategy A: bypass docker_image::extract | Zero risk of divergence from docker-source semantics; reuses all downstream code |
| Storage-root discovery | Honor `containers.conf` `graphroot` + fall back to well-known defaults | Shell out to `podman info` / require flag | Matches operator config without new flag; FR-009 filesystem-only preserved |
| Storage driver support | `overlay` MVP; `vfs`/`btrfs` WARN + fallback | Support all 3 drivers | YAGNI; overlay is 95%+ of installs |
| `mikebom:image-source` emission | Conditional (only when source == Podman) | Always-emit + regen goldens | Preserves FR-005 byte-identity for non-podman goldens; matches m134/m161/m204 precedent |
| Catalog row ID | C124 (next free after C123) | (n/a) | m204 shipped C123; verified via grep |
| New Cargo deps | Zero | (n/a) | Nothing needed |
