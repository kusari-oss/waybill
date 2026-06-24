# Contract — `pkg:composer/*` and `pkg:generic/*` component PURL

The only wire-format contract this feature introduces. Per Constitution Principle V audit (research §R1):

- `pkg:composer/` is **purl-spec-blessed** ([composer-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md)) — used for packagist, vcs, composer-plugin, metapackage source types.
- `pkg:generic/` is the placeholder for path source type (purl-spec does not define a `composer-path` type).
- The source-type discriminator surfaces via the existing parity-catalog C1 row (`mikebom:source-type` annotation) — no new C-row added; Composer contributes new VALUES (`composer-packagist` / `composer-vcs` / `composer-path` / `composer-plugin` / `composer-metapackage` / `composer-main-module`) to C1's value set.
- The `mikebom:lockfile-orphan` annotation (Q1 clarification) is a NEW property emission. Deferred parity-catalog refresh; lives as `extra_annotations` data without blocking this milestone.

## Wire shapes per source

### Packagist (default)

```text
pkg:composer/<lc-vendor>/<lc-package>@<version>
```

Vendor + package segments lowercased per purl-spec canonical form. Version preserved verbatim (including `v` prefix for git-tag versions like `v7.0.4`).

### Packagist (self-hosted mirror)

```text
pkg:composer/<lc-vendor>/<lc-package>@<version>?repository_url=<base-url-with-scheme>
```

`?repository_url=` qualifier omitted when `dist.url` base matches any of:
- `https://packagist.org` (canonical default)
- `https://repo.packagist.org` (the API host that real-world lockfiles use)
- `https://api.github.com/repos/` (default Packagist's redirect target for dist downloads)

### VCS (git / svn / hg)

```text
pkg:composer/<lc-vendor>/<lc-package>@<version>?vcs_url=<scheme>+<vcs-remote-url>
```

- `vcs_url` value carries the appropriate scheme prefix (`git+` for `source.type: git`; `svn+` for svn; `hg+` for hg) per the cross-type purl-spec convention.
- Version segment uses the lockfile's `version:` field verbatim (NOT the resolved SHA — Composer records the upstream tag-or-branch in `version:` even for VCS sources, so PURL version conveys real upstream identity).
- The resolved SHA from `source.reference` is preserved as `mikebom:vcs-ref` evidence annotation, not in the PURL.

### Path

```text
pkg:generic/<lc-vendor>-<lc-package>@<version>
```

Vendor + name flattened with `-` because `pkg:generic/` doesn't support the namespace split. Plus `mikebom:source-type = "composer-path"` annotation as discriminator. Plus `mikebom:path = "<source.url>"` annotation preserving the relative-or-absolute path verbatim.

### Composer plugin (`type: composer-plugin` or legacy `composer-installer`)

```text
pkg:composer/<lc-vendor>/<lc-package>@<version>
```

Standard Packagist PURL form — these ARE Packagist-addressable. Plus `mikebom:source-type = "composer-plugin"` + `mikebom:composer-type = "<type-field-verbatim>"` annotations so consumers can distinguish modern `composer-plugin` from legacy `composer-installer`.

### Metapackage (`type: metapackage`)

```text
pkg:composer/<lc-vendor>/<lc-package>@<version>
```

Standard Packagist PURL form. Plus `mikebom:source-type = "composer-metapackage"` annotation. Metapackages have no downloadable artifact (no `dist.shasum`); `hashes` is empty.

### Main-module (per FR-012)

```text
pkg:composer/<lc-vendor>/<lc-package>@<composer.json.version-or-"0.0.0-unknown">
```

Plus `mikebom:component-role = "main-module"` + `mikebom:source-type = "composer-main-module"` annotations. Skipped when `composer.json` lacks `name:` field (per Q3); deps still emit.

## Examples

| Scan input | Emitted PURL |
|---|---|
| `composer.json`: `"name": "acme/my-app"`, `"version": "1.2.3"` | `pkg:composer/acme/my-app@1.2.3` (main-module) |
| `composer.lock`: `symfony/console` from `packagist.org`, version `v7.0.4`, dist.shasum `abc...` | `pkg:composer/symfony/console@v7.0.4` (hashes: `[sha1:abc...]`) |
| `composer.lock`: `acme/internal_lib` from `https://repo.acme.example.com`, version `2.0.0` | `pkg:composer/acme/internal_lib@2.0.0?repository_url=https://repo.acme.example.com` |
| `composer.lock`: `acme/my-fork` source.type=git, source.url=`https://github.com/acme/my-fork.git`, source.reference=`eb39649...`, version `dev-main` | `pkg:composer/acme/my-fork@dev-main?vcs_url=git+https://github.com/acme/my-fork.git` (annotation: `mikebom:vcs-ref = "eb39649..."`) |
| `composer.lock`: `acme/local-lib` source.type=path, source.url=`../packages/local-lib`, version `0.1.0` | `pkg:generic/acme-local-lib@0.1.0` (annotations: `mikebom:source-type = "composer-path"`, `mikebom:path = "../packages/local-lib"`) |
| `composer.lock`: `composer/installers` type=composer-plugin, version `v2.3.0` | `pkg:composer/composer/installers@v2.3.0` (annotations: `mikebom:source-type = "composer-plugin"`, `mikebom:composer-type = "composer-plugin"`) |
| `composer.lock`: `symfony/symfony` type=metapackage, version `v7.0.4` | `pkg:composer/symfony/symfony@v7.0.4` (annotation: `mikebom:source-type = "composer-metapackage"`) |
| `installed.json` entry `acme/orphan` not in sibling lockfile (sibling lockfile EXISTS) | `pkg:composer/acme/orphan@<version>` (annotations: `mikebom:sbom-tier = "deployed"`, `mikebom:lockfile-orphan = "true"`) |
| `installed.json` entry in a deployed-only scan (no sibling lockfile) | `pkg:composer/<vendor>/<package>@<version>` (annotation: `mikebom:sbom-tier = "deployed"`; NO `mikebom:lockfile-orphan` annotation) |

## Per-format emission

### CycloneDX 1.6

Location: `.components[].purl` (native).

```json
{
  "type": "library",
  "name": "symfony/console",
  "version": "v7.0.4",
  "purl": "pkg:composer/symfony/console@v7.0.4",
  "hashes": [
    {"alg": "SHA-1", "content": "<lowercase-hex-from-lockfile-dist.shasum>"}
  ],
  "properties": [
    {"name": "mikebom:source-type", "value": "composer-packagist"},
    {"name": "mikebom:evidence-kind", "value": "composer-lock"},
    {"name": "mikebom:sbom-tier", "value": "source"}
  ]
}
```

The `mikebom:source-type` / `mikebom:evidence-kind` / `mikebom:sbom-tier` properties are existing per-component annotations. The PURL + SHA-1 hash + `mikebom:lockfile-orphan` (when applicable) are the only new wire-format additions.

### SPDX 2.3

Location: `.packages[].externalRefs[]` with `referenceCategory: PACKAGE-MANAGER`.

```json
{
  "name": "symfony/console",
  "versionInfo": "v7.0.4",
  "externalRefs": [
    {
      "referenceCategory": "PACKAGE-MANAGER",
      "referenceType": "purl",
      "referenceLocator": "pkg:composer/symfony/console@v7.0.4"
    }
  ],
  "checksums": [
    {"algorithm": "SHA1", "checksumValue": "<hex>"}
  ],
  "annotations": [
    { /* mikebom:source-type / evidence-kind / sbom-tier envelopes via existing annotation pattern */ }
  ]
}
```

### SPDX 3.0.1

Location: `software_Package.software_packageUrl` + `Element.externalIdentifier[]`.

```json
{
  "type": "software_Package",
  "spdxId": "...",
  "name": "symfony/console",
  "software_packageVersion": "v7.0.4",
  "software_packageUrl": "pkg:composer/symfony/console@v7.0.4",
  "externalIdentifier": [
    {
      "type": "ExternalIdentifier",
      "externalIdentifierType": "packageUrl",
      "identifier": "pkg:composer/symfony/console@v7.0.4"
    }
  ]
}
```

## Determinism

For a given `composer.lock` / `composer.json` / `installed.json`, the emitted PURL set MUST be identical across runs:

- Lockfile / installed entries processed in their JSON-array order (preserved by `serde_json`).
- Main-module components processed in walker discovery order (sorted directory entries per `safe_walk` convention from milestone 114).
- `extra_annotations` `BTreeMap` ensures deterministic property emission order.

## Absence semantics

When the scanned root contains none of `composer.lock` / `composer.json` / `vendor/composer/installed.json`:

- Zero `pkg:composer/*` and zero `pkg:generic/*` Composer-derived components emit.
- No warnings fire (per FR-007).
- SBOM bytes are identical (modulo timestamps + serial numbers) to a pre-feature scan (SC-004 invariant).

## Parity-catalog note

Because the wire-format addition is the native PURL field (not a new `mikebom:*` annotation), no new C-row is added to `docs/reference/sbom-format-mapping.md` for identity. The PURL surfaces via the existing A1 row ("PURL").

The `mikebom:source-type` annotation reuses C1 (introduced in milestone 002 for cargo's path/git/registry discrimination); Composer contributes new VALUES (`composer-packagist` / `composer-vcs` / `composer-path` / `composer-plugin` / `composer-metapackage` / `composer-main-module`) to C1's value set without altering wire shape.

The new `mikebom:lockfile-orphan` annotation per Q1 is a new C-row candidate; deferred to a follow-up parity-catalog refresh (it lives as `extra_annotations` data in v1, surfaces via the same JSON-property mechanism the catalog already documents for cross-cutting `extra_annotations` emission).
