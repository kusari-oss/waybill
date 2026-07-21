# Cross-tier SBOM correlation walk

A worked example demonstrating **content-addressable cross-tier correlation** across waybill-emitted source / build / image SBOMs — without invoking waybill.

The point: once waybill has emitted SBOMs at each tier with the identifier substrate from milestones 072–076, an external tool with **only** standards-native SBOM parsing (no waybill dependency, no waybill-specific plugin) can walk:

```
image SBOM → build SBOM → source SBOM
```

purely by string-match on the identifiers + content hashes that appear in every emitted document.

## What's in this directory

```
docs/examples/cross-tier-walk/
├── README.md           # This file
├── walk.py             # ~150-line stdlib-only Python walker
└── sboms/
    ├── source.cdx.json  # Synthetic source-tier SBOM for acme/widget-svc
    ├── build.cdx.json   # Synthetic build-tier SBOM (subject hashes inside)
    └── image.cdx.json   # Synthetic image-tier SBOM (component hashes inside)
```

The three SBOMs describe a fictional Go service `acme/widget-svc` at three lifecycle tiers. Hand-crafted (not generated) so each fits on a screen and the cross-tier links are visually obvious.

## Run the walker

```bash
$ python3 docs/examples/cross-tier-walk/walk.py docs/examples/cross-tier-walk/sboms/
image  : acme/widget-svc           ← image.cdx.json
  via  : image-digest handshake
build  : widget-svc                ← build.cdx.json
  via  : repo:git@github.com:acme/widget-svc.git
source : widget-svc                ← source.cdx.json
```

The walker:

1. Loads every `*.cdx.json` file in the directory.
2. Indexes each SBOM by its document-level identifiers — `repo:`, `git:`, `image:`, `subject:`, `attestation:`. The schemes are extracted from `metadata.component.externalReferences[]` (CDX's standards-native identifier carrier).
3. For each image-tier SBOM, walks: image's `image:` digest → matching build's `subject:` → matching source's `repo:`.
4. Prints the chain in plain-text.

**No waybill dependency. No new schema. Just CDX 1.6 + Python stdlib.**

## How the cross-tier links work

### Mode 1 — image-digest handshake (what fires on the example)

The build SBOM emits one `subject:` identifier per artifact the build produced. When the build wraps `docker build -t foo:v1 .`, one of those subjects is the resulting image manifest digest. The image scan (`waybill sbom scan --image foo:v1`) auto-detects `image:foo:v1@sha256:DIGEST`. The DIGEST portion is byte-identical between the two — waybill didn't coordinate; it's just the content hash of the same OCI manifest, observed from two angles.

In our example:

| SBOM | Identifier | Hex |
|------|------------|-----|
| `build.cdx.json` | `subject:sha256:1111aaaa...` | `1111aaaa2222bbbb3333cccc4444dddd5555eeee66667777888899990000aaaa` |
| `image.cdx.json` | `image:acme/widget-svc:v1@sha256:1111aaaa...` | (same hex, in the digest portion) |

The walker matches by string equality on the hex.

### Mode 2 — per-component walk (fallback when no image-level subject)

Not every build produces an image. When the build produces a binary (e.g., `cargo install ripgrep`), the build SBOM's `subject:sha256:X` is the binary's hash. The image SBOM lists components with their content hashes; one of those `components[].hashes[]` entries equals X. The walker matches per-component when the image-level handshake doesn't find a build.

Our example exercises this too — the build emits **two** subjects (binary + image manifest), and the image SBOM's `widget-svc` component carries the binary's hash:

| SBOM | Identifier / hash | Hex |
|------|-------------------|-----|
| `build.cdx.json` | `subject:sha256:aaaa1111...` (binary) | `aaaa1111bbbb2222cccc3333dddd4444eeee5555ffff66667777888899990000` |
| `image.cdx.json` | `components[].hashes[].sha256` on `widget-svc` | (same hex) |

Mode 1 fires first in the walker, so the per-component walk is dormant in this example. To see it fire, comment out the image-manifest subject in `build.cdx.json` and re-run.

### Source link (build → source)

The build SBOM auto-detects `repo:git@github.com:acme/widget-svc.git` from `git remote get-url origin` (milestone 074). The source SBOM auto-detects the same value from the same git config (milestone 073). Match by string equality on the URL.

## Wire format reference (CDX 1.6)

All identifiers ride **standards-native** CDX fields. Constitution Principle V: zero `waybill:*` annotations involved in the walk.

| Identifier | CDX carrier | Example |
|------------|-------------|---------|
| `repo:` | `metadata.component.externalReferences[type:vcs]` | `{"type": "vcs", "url": "git@github.com:acme/widget-svc.git", "comment": "auto-detected from git remote `origin`"}` |
| `git:` | `metadata.component.externalReferences[type:vcs]` | URL contains `#<commit-sha>` fragment |
| `image:` | `metadata.component.externalReferences[type:distribution]` | `{"type": "distribution", "url": "acme/widget-svc:v1@sha256:1111aaaa..."}` |
| `subject:` | `metadata.component.externalReferences[type:attestation]` | URL is `<algo>:<hex>` form (distinguishes from `attestation:` IRIs) |
| `attestation:` | `metadata.component.externalReferences[type:attestation]` | URL is an IRI (`https://...`) |
| Per-component user-defined | `components[].properties[]` | `{"name": "kusari-id", "value": "asset-foo"}` |

Equivalent SPDX 2.3 mappings ride `Package.externalRefs[PERSISTENT-ID]`; SPDX 3 mappings ride `Element.externalIdentifier[]`. See [`docs/reference/identifiers.md`](../../reference/identifiers.md) for the full per-format wire mapping.

## jq cheatsheet — same walk in shell

The Python walker is the readable reference. For one-liners on a single SBOM:

```bash
# Document-level identifiers (any tier)
jq '.metadata.component.externalReferences[] | {type, url, comment}' sboms/build.cdx.json

# Just the subject digests on a build SBOM
jq -r '.metadata.component.externalReferences[]
       | select(.type == "attestation")
       | select(.url | startswith("sha256:") or startswith("sha512:"))
       | .url' sboms/build.cdx.json

# Per-component hashes from an image SBOM
jq '.components[] | select(.hashes) | {purl, sha256: (.hashes[] | select(.alg == "SHA-256") | .content)}' sboms/image.cdx.json

# Mode 1 handshake: image's image: digest
jq -r '.metadata.component.externalReferences[]
       | select(.type == "distribution")
       | .url
       | split("@")[1]' sboms/image.cdx.json
# → sha256:1111aaaa2222bbbb3333cccc4444dddd5555eeee66667777888899990000aaaa
```

## Generating your own example with waybill

Substitute your own project. The same walker works on real SBOMs:

```bash
cd ~/projects/my-app

# Source-tier (auto-detects `repo:`)
waybill sbom scan --path . --output /tmp/sboms/source.cdx.json

# Build-tier (Linux + eBPF + privileges only; auto-detects `repo:`/`git:`/`subject:`)
waybill trace run --signing-key ./key \
    --sbom-output /tmp/sboms/build.cdx.json \
    --attestation-output /tmp/sboms/build.attestation.dsse.json \
    -- docker build -t my-app:v1 .

# Image-tier (auto-detects `image:`)
waybill sbom scan --image my-app:v1 --output /tmp/sboms/image.cdx.json

# Walk
python3 docs/examples/cross-tier-walk/walk.py /tmp/sboms/
```

The walker is tier-agnostic — it just finds image-tier SBOMs by the `waybill:sbom-tier` property and walks from each. Drop more SBOMs into the directory, re-run; previously-correlated chains stay correlated, new ones light up.

## What this demo proves

- waybill emits identifiers at every tier in standards-native carriers.
- An external tool with **150 lines of stdlib Python** (and zero waybill-specific knowledge beyond "look at `externalReferences[]`") can recover the full source/build/image chain by content-addressable string match.
- The chain works even when the SBOMs don't reference each other — the correlation is purely a property of the inputs flowing through (binary hash, image manifest digest, git remote URL).
- Waybill's `--bind-to-source` (milestone 072) is **complementary**, not required: it adds a cryptographic embedded reference for stronger guarantees, but the content-addressable walk demonstrated here works without any explicit bindings.

## What this demo does NOT cover

- **Build attestation envelope**: the `*.attestation.dsse.json` file produced by `waybill trace run` carries the signed in-toto statement. The walker here works on the SBOM body alone (the `subject:` identifier is duplicated from the attestation envelope into the SBOM body for exactly this purpose). Verifying the attestation signature is a separate flow — see [`docs/architecture/attestations.md`](../../architecture/attestations.md) and `waybill sbom verify`.
- **VEX / vulnerability data**: the cross-tier walk surfaces "which build/source produced this binary"; it doesn't surface vulnerabilities. waybill's VEX support lives in milestone 072's PR-B work; the walker could be extended to follow VEX links similarly.
- **Multi-source builds**: an image built from multiple repos would have multiple `repo:` identifiers on its build SBOM. The walker as written matches the first; extending to "find ALL source SBOMs whose `repo:` matches ANY of the build's `repo:` entries" is a 5-line edit.
