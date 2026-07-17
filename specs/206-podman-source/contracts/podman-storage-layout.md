# Contract: Podman `containers/storage` Overlay Layout

**Date**: 2026-07-17
**Purpose**: Pin the on-disk layout mikebom parses. Podman uses the `containers/storage` library (github.com/containers/storage); mikebom is a read-only consumer. Layout stable across podman v4.x + v5.x for the `overlay` driver (m206 MVP scope).

## Storage root

`<graphroot>` — configurable, default:
- Rootless (per-user, non-zero euid): `$HOME/.local/share/containers/storage/`
- Rootful (euid == 0): `/var/lib/containers/storage/`
- Override via `storage.conf` `[storage] graphroot = "..."` key (per FR-010 + research R2).

## Directory tree mikebom reads (overlay driver only)

```
<graphroot>/
├── overlay-images/
│   ├── images.json                   # Top-level image index — see §Image Index
│   ├── <image-id>/
│   │   ├── manifest                  # OCI ImageManifest (JSON)
│   │   ├── config                    # OCI ImageConfiguration (JSON)
│   │   ├── metadata                  # c/storage internal (ignored by mikebom)
│   │   └── =0                        # c/storage internal marker (ignored)
│   └── ...
├── overlay-layers/
│   ├── layers.json                   # Top-level layer index — see §Layer Index
│   └── <layer-id>/                   # c/storage internal (ignored by mikebom directly)
├── overlay/
│   ├── <layer-id>/
│   │   ├── diff/                     # Layer contents (unpacked) — see §Layer Content
│   │   ├── link                      # Overlay short-name (ignored)
│   │   ├── lower                     # Overlay lower-dir list (ignored)
│   │   └── work/                     # Overlay work-dir (ignored)
│   └── ...
└── storage.conf                      # c/storage config (may override graphroot etc.)
```

## Image index — `overlay-images/images.json`

**Wire shape** (JSON array of image records):

```json
[
  {
    "id": "abc123def456...64-hex-chars",
    "digest": "sha256:...",
    "names": ["nginx:1.27.0", "docker.io/library/nginx:1.27.0"],
    "layer": "layer-id-of-topmost-layer",
    "metadata": "...",
    "big-data-names": ["manifest", "config"],
    "big-data-sizes": {"manifest": 1234, "config": 5678},
    "created": "2026-07-01T00:00:00Z",
    "read-only": false
  }
]
```

**mikebom uses**:
- `id` — c/storage internal image ID (64-hex). Used to locate `<graphroot>/overlay-images/<id>/{manifest,config}`.
- `digest` — OCI content digest (sha256). Used for `PodmanImageRef::Digest` matching.
- `names` — the operator-friendly tag list. Used for `PodmanImageRef::Tagged` matching (exact-string match; handles both `nginx:1.27.0` and `docker.io/library/nginx:1.27.0` forms).
- `layer` — topmost layer's c/storage internal ID. Chain resolved via `layers.json` (see below).

**mikebom ignores**: `big-data-*`, `created`, `read-only`, `metadata` — not needed for rootfs assembly.

## Layer index — `overlay-layers/layers.json`

**Wire shape** (JSON array):

```json
[
  {
    "id": "layer-id-64-hex",
    "parent": "parent-layer-id-or-empty",
    "diff-digest": "sha256:abc...",
    "diff-size": 123456,
    "compressed-diff-digest": "sha256:def...",
    "compressed-size": 12345,
    "created": "2026-07-01T00:00:00Z"
  }
]
```

**mikebom uses**:
- `id` — internal layer ID; maps to `<graphroot>/overlay/<id>/diff/` for content.
- `parent` — chain traversal: walk from topmost layer (per image record) up to the base (empty parent).
- `diff-digest` — SHA-256 of the UNCOMPRESSED tar of the diff dir. Used to verify content integrity against the OCI manifest's declared layer digests (FR-012).
- `compressed-diff-digest` — SHA-256 of the compressed .tar.gz. Used for the assembler input (matches what the OCI manifest declares).

**Chain resolution algorithm**:

```
layers = []
current = image.layer
while current != "":
    entry = layers_json[current]
    layers.push(entry)
    current = entry.parent
layers.reverse()  // base-to-top ordering matches docker-save convention
```

## Layer content — `overlay/<layer-id>/diff/`

**Wire shape**: An ordinary directory tree containing the layer's UNPACKED files. Podman's overlay driver stores the diff (not the compressed blob).

**mikebom uses**: for each layer in the resolved chain, walk `<graphroot>/overlay/<layer-id>/diff/` with `walkdir::WalkDir` and re-pack into a `tar::Builder` → gzip → in-memory `Vec<u8>` blob → hand to `oci_pull::tarball::assemble_docker_save_tarball` as a `PulledLayer`.

**Digest verification** (per FR-012):
- Compute SHA-256 of the uncompressed tar → MUST match `layers.json[<id>].diff-digest`.
- Compute SHA-256 of the compressed .tar.gz → MUST match `layers.json[<id>].compressed-diff-digest` OR the OCI manifest's declared layer digest for that layer position.
- Mismatch → return `PodmanSourceError::LayerDigestMismatch` → error propagated (no fallback; integrity is non-recoverable per m206 posture).

**Note on tar reproducibility**: repacking a directory into a tar may produce different byte output than the original .tar.gz depending on file ordering + mtime handling. mikebom must sort directory entries lexicographically + zero mtimes for byte-reproducibility across scans. Standard `tar` crate options handle this: `Builder::mode(HeaderMode::Deterministic)`.

## `storage.conf` — c/storage config

**Location**:
- Rootful: `/etc/containers/storage.conf`
- Rootless: `$HOME/.config/containers/storage.conf`

**Wire shape** (TOML):

```toml
[storage]
driver = "overlay"
graphroot = "/mnt/podman-storage"   # override
runroot = "/run/containers/storage"  # ignored by mikebom (runtime state, not persistent images)

[storage.options.overlay]
mountopt = "nodev,metacopy=on"       # ignored — mikebom doesn't mount, only reads diff dirs directly
```

**mikebom uses**: only `[storage] driver` (must be `"overlay"` for m206) and `[storage] graphroot`. Everything else is ignored.

**Absent file**: fall back to compiled-in defaults per §Storage root.

## Non-goals for m206 MVP (deferred to follow-ups)

- **`vfs` storage driver**: `<graphroot>/vfs/dir/<layer-id>/`. Layout differs — no separate `diff/` subdir. Would need a per-driver adapter.
- **`btrfs` storage driver**: `<graphroot>/btrfs/subvolumes/<layer-id>/`. Subvolume-per-layer; snapshot-based deltas. Would need btrfs-specific tooling to walk deltas.
- **Additional container storage roots via `additionalimagestores`** (multi-store setups where images span multiple graphroots). Read-only extra stores are a real pattern in shared CI runners but out of scope for m206.
- **Podman REST API**: `unix:///run/user/<uid>/podman/podman.sock`. Would need `hyper-util` or similar unix-socket HTTP client. FR-009 defers explicitly.
- **`podman machine` VM introspection on macOS/Windows**: podman on non-Linux runs a Linux VM; its storage lives inside the VM. Reaching in would need VM API integration. Deferred per spec Assumption 1.
