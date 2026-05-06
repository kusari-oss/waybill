# Quickstart — milestone 077 root component override

Five operator-facing recipes covering the `--root-name` and `--root-version` flags.

## Recipe 1 — The headline: override on a generic source-tier scan

The use case the milestone exists for. An operator points mikebom at an arbitrary directory whose basename doesn't reflect the operator-meaningful project identity.

```bash
# Scan an arbitrary build artifact tree
mikebom sbom scan --path /opt/builds/abc123-snapshot \
    --root-name widget-svc --root-version 1.2.3 \
    --output widget-svc.cdx.json
# INFO root component override active: name='widget-svc' (replacing 'abc123-snapshot'), version='1.2.3' (replacing '0.0.0')
```

Inspect the emitted SBOM:

```bash
jq '.metadata.component | {name, version, "bom-ref", purl, cpe}' widget-svc.cdx.json
# {
#   "name": "widget-svc",
#   "version": "1.2.3",
#   "bom-ref": "widget-svc@1.2.3",
#   "purl": "pkg:generic/widget-svc@1.2.3",
#   "cpe": "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"
# }
```

All derived fields (`bom-ref`, `purl`, `cpe`) follow the override automatically — no separate flags needed for them.

## Recipe 2 — One flag at a time

The flags are independent. Operator can override just the name (keeping the auto-derived version), or just the version (keeping the basename-derived name):

```bash
# Just override the name
mikebom sbom scan --path /opt/builds/abc123-snapshot --root-name widget-svc --output a.cdx.json
jq '.metadata.component | {name, version}' a.cdx.json
# {"name": "widget-svc", "version": "0.0.0"}

# Just override the version
mikebom sbom scan --path /opt/builds/abc123-snapshot --root-version 1.2.3 --output b.cdx.json
jq '.metadata.component | {name, version}' b.cdx.json
# {"name": "abc123-snapshot", "version": "1.2.3"}
```

## Recipe 3 — Override on a manifest-driven Cargo project

Operator wants to publish an SBOM identifying the project as `widget-svc@1.2.3` even though Cargo's `[package].name = "foo-internal"`. The override is a **clean replacement** — the manifest-derived `foo-internal` identity is dropped entirely (not demoted to `components[]`).

```bash
cd ~/projects/foo-internal-cargo  # has Cargo.toml with [package].name = "foo-internal"
mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 --output out.cdx.json
# INFO root component override active: name='widget-svc' (replacing 'foo-internal'), version='1.2.3' (replacing '0.5.1')
# INFO override is set; dropping manifest-derived main-module component `pkg:cargo/foo-internal@0.5.1` from emitted SBOM (per milestone 077 clean-replacement clarification; see GitHub issue #151 for the demote-to-library follow-up)
```

Verify:

```bash
jq '.metadata.component.name' out.cdx.json
# "widget-svc"

# The Cargo main-module is gone:
jq '.components[] | select(.purl == "pkg:cargo/foo-internal@0.5.1")' out.cdx.json
# (no output)

# Cargo dependencies still present:
jq '.components | length' out.cdx.json
# 42  (or whatever — deps count, minus the dropped main-module)
```

If you want the manifest-derived `foo-internal` identity preserved as a library entry alongside the override, that's tracked as future work in **GitHub issue #151**. Today's MVP uses clean replacement. If you need it now, post-process with `mikebom sbom enrich` to add the manifest entry back.

## Recipe 4 — Override on image-tier scan

The same flags work on `mikebom sbom scan --image`. Useful when the operator wants the image SBOM to identify as the deployed service name rather than the image basename.

```bash
mikebom sbom scan --image acme/internal-bld:v1 \
    --root-name widget-svc-image --root-version 1.2.3 \
    --output image.cdx.json
# INFO root component override active: name='widget-svc-image' (replacing 'acme/internal-bld'), version='1.2.3' (replacing 'v1')
```

The auto-detected `image:` identifier (milestone 073) is unaffected — it's a separate slot in `externalReferences[]`:

```bash
jq '.metadata.component | {name, version, externalReferences: [.externalReferences[] | select(.type == "distribution")]}' image.cdx.json
# {
#   "name": "widget-svc-image",
#   "version": "1.2.3",
#   "externalReferences": [
#     {
#       "type": "distribution",
#       "url": "acme/internal-bld:v1@sha256:abc...",
#       "comment": "auto-detected from resolved image reference"
#     }
#   ]
# }
```

The root component identity is operator-controlled; the `image:` identifier still records what was actually scanned.

## Recipe 5 — Override + identifiers + binding (full layered example)

The override flag is **orthogonal** to all milestone-072–076 identifier and binding flags. Stack them freely:

```bash
mikebom sbom scan --path . \
    --root-name widget-svc --root-version 1.2.3 \
    --repo git@github.com:acme/widget-svc.git \
    --git-ref abc1234567890abcdef1234567890abcdef12345 \
    --subject-hash sha256:def5678901234567890abcdef1234567890abcdef1234567890abcdef12345aa \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --output out.cdx.json
```

The emitted SBOM has:
- Root component identity from `--root-name` / `--root-version` (this milestone)
- Auto-detected + manual identifiers from milestones 073/074/076 in `externalReferences[]`
- Per-component user-defined identifier on the matching `serde` component (milestone 076)
- All independent slots; all visible to external consumers via the standards-native carriers documented in `docs/reference/identifiers.md`

## Recipe (validation) — npm-scoped names work

Per the 2026-05-06 permissive-validation clarification, names like `@acme/widget-svc` are accepted (no rejection at parse), and URL-encoded into the PURL `name` segment automatically:

```bash
mikebom sbom scan --path . --root-name "@acme/widget-svc" --root-version 1.2.3 --output out.cdx.json

jq '.metadata.component | {name, purl, "bom-ref"}' out.cdx.json
# {
#   "name": "@acme/widget-svc",                              # ← name field preserves operator's exact value
#   "purl": "pkg:generic/%40acme%2Fwidget-svc@1.2.3",        # ← PURL has URL-encoded name segment
#   "bom-ref": "@acme/widget-svc@1.2.3"                      # ← bom-ref also preserves verbatim
# }
```

## Recipe (validation) — what gets rejected

Names with whitespace, control chars, `?`, or `#` fail at CLI parse before any scan work happens:

```bash
mikebom sbom scan --path . --root-name "my widget svc"
# error: invalid value 'my widget svc' for '--root-name <NAME>': --root-name contains whitespace at position 2 (character: ' '); whitespace is not allowed

mikebom sbom scan --path . --root-name ""
# error: invalid value '' for '--root-name <NAME>': --root-name must not be empty

mikebom sbom scan --path . --root-name "foo?bar"
# error: invalid value 'foo?bar' for '--root-name <NAME>': --root-name contains URL-syntax-breaking character '?' at position 3; '?' and '#' are not allowed
```

Versions follow the same rule:

```bash
mikebom sbom scan --path . --root-version "1.2 .3"
# error: invalid value '1.2 .3' for '--root-version <VERSION>': --root-version contains whitespace at position 3 (character: ' '); whitespace is not allowed
```

## When to use each layer of identity (recap)

After milestones 072–077, mikebom has **four layers of identity** in an emitted SBOM. Use them together based on what the operator needs to express:

| Layer | Surface | What to use |
|-------|---------|-------------|
| 1. Root component identity | `metadata.component.{name,version,bom-ref,purl,cpe}` | `--root-name` / `--root-version` (077) — overrides auto-derived defaults |
| 2. Document-level identifiers | `metadata.component.externalReferences[]` | `--repo` / `--git-ref` / `--image-id` / `--attestation` / `--subject-hash` / `--id <scheme>=<value>` (073–076) — cross-tier correlation handles |
| 3. Per-component identifiers | `components[].properties[]` | `--component-id <PURL>=<scheme>:<value>` (076) — per-component user-defined |
| 4. Cross-tier binding | `mikebom:source-document-binding` annotation | `--bind-to-source <PATH>` (072) — cryptographic embedded reference |

This milestone (077) closes the operator-visible gap at Layer 1. Layers 2–4 were closed by 072–076.
