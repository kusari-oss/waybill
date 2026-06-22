# Contract — `pkg:alpm/*` component PURL

The only wire-format contract this feature introduces. Per Constitution Principle V audit (research §R1), the alpm reader emits NO `mikebom:*` annotation — the PURL itself IS the standards-native component identity, and CycloneDX 1.6, SPDX 2.3, and SPDX 3.0.1 all consume PURLs as a first-class identity carrier.

## Wire shape

```
pkg:alpm/<distro-namespace>/<package-name>@<version>?arch=<architecture>[&distro=<namespace>-<version-id>]
```

### Segments

| Segment | Source | Encoding | Examples |
|---|---|---|---|
| `pkg:alpm/` | Fixed | Verbatim | `pkg:alpm/` |
| `<distro-namespace>/` | `/etc/os-release` `ID` (lowercased) — defaults to `arch` | Percent-encoded per PURL spec | `arch/`, `manjaro/`, `steamos/`, `endeavouros/`, `cachyos/` |
| `<package-name>` | `%NAME%` from `desc` | Percent-encoded per PURL spec | `bash`, `linux`, `python-requests`, `lib32-glibc` |
| `@<version>` | `%VERSION%` from `desc` | Percent-encoded per PURL spec; preserves `<upstream>-<pkgrel>` form verbatim | `@5.2.026-1`, `@2.40-1`, `@8.5.0-2.1` |
| `?arch=<architecture>` | `%ARCH%` from `desc` — always present | Verbatim | `?arch=x86_64`, `?arch=aarch64`, `?arch=any` |
| `&distro=<namespace>-<version-id>` | `/etc/os-release` `ID` + `VERSION_ID` when both present; **omitted entirely on rolling-release distros without `VERSION_ID`** | Verbatim | `&distro=steamos-3.5.7`, `&distro=manjaro-24.0.0` |

## Examples

| Scan target | Emitted PURL |
|---|---|
| Stock Arch container (`archlinux:latest`), `bash` 5.2.026-1 | `pkg:alpm/arch/bash@5.2.026-1?arch=x86_64` |
| Stock Arch, noarch `terminfo` | `pkg:alpm/arch/terminfo@6.4_p20230819-3?arch=any` |
| Stock Arch, `lib32-glibc` (multilib) | `pkg:alpm/arch/lib32-glibc@2.40-1?arch=x86_64` |
| SteamOS rootfs, `bash` | `pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7` |
| Manjaro install, `bash` | `pkg:alpm/manjaro/bash@5.2.026-1?arch=x86_64&distro=manjaro-24.0.0` |
| EndeavourOS rootfs (rolling, no `VERSION_ID`), `bash` | `pkg:alpm/endeavouros/bash@5.2.026-1?arch=x86_64` |
| CachyOS, `bash` | `pkg:alpm/cachyos/bash@5.2.026-1?arch=x86_64[&distro=cachyos-<ver>]` (qualifier present only when `VERSION_ID` is present in the scanned rootfs) |
| Unknown derivative with `/etc/os-release` `ID=mydistro` | `pkg:alpm/mydistro/bash@5.2.026-1?arch=x86_64` |

## Per-format emission

### CycloneDX 1.6

Location: `.components[].purl` (native).

```json
{
  "type": "library",
  "name": "bash",
  "version": "5.2.026-1",
  "purl": "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64",
  "properties": [
    { "name": "mikebom:source-type", "value": "alpm" },
    { "name": "mikebom:evidence-kind", "value": "alpm-local-db" },
    { "name": "mikebom:sbom-tier", "value": "deployed" }
  ]
}
```

The `mikebom:source-type` / `mikebom:evidence-kind` / `mikebom:sbom-tier` properties shown above are NOT new — they're the existing per-component annotations emitted by every package-DB reader (see milestone 002 for source-type, milestone 004 for evidence-kind, milestone 002 for sbom-tier). The PURL is the only new wire-format addition.

### SPDX 2.3

Location: `.packages[].externalRefs[]` with `referenceCategory: PACKAGE-MANAGER`.

```json
{
  "name": "bash",
  "versionInfo": "5.2.026-1",
  "supplier": "Person: Levente Polyak <anthraxx@archlinux.org>",
  "externalRefs": [
    {
      "referenceCategory": "PACKAGE-MANAGER",
      "referenceType": "purl",
      "referenceLocator": "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64"
    }
  ],
  "annotations": [
    { /* the standard mikebom:source-type / evidence-kind / sbom-tier envelope rides here */ }
  ]
}
```

### SPDX 3.0.1

Location: `software_Package.software_packageUrl` + `Element.externalIdentifier[]` with `externalIdentifierType: "packageUrl"`.

```json
{
  "type": "software_Package",
  "spdxId": "...",
  "name": "bash",
  "software_packageVersion": "5.2.026-1",
  "software_packageUrl": "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64",
  "externalIdentifier": [
    {
      "type": "ExternalIdentifier",
      "externalIdentifierType": "packageUrl",
      "identifier": "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64"
    }
  ]
}
```

## Determinism

For a given on-disk pacman DB and `/etc/os-release` content, the emitted PURL set MUST be identical across runs:

- Component order follows pacman DB walk order (sorted directory entries — `/var/lib/pacman/local/*` enumerated lex-ascending).
- Within a stanza, multi-value field order (e.g., `depends`) follows `desc`-file declaration order.
- Qualifiers (`arch=`, `distro=`) appear in sorted-key order per PURL spec.

## Absence semantics

When the scanned rootfs contains no pacman DB (no `/var/lib/pacman/local/` or that directory is empty):

- Zero alpm-derived components emit.
- No warnings fire (per FR-008).
- The SBOM document's serialized bytes are identical (modulo timestamps + serial numbers) to a pre-feature scan of the same rootfs (SC-003 invariant).

## Parity-catalog note

Because the wire-format addition is a native PURL (not a `mikebom:*` annotation), no new C-row is added to `docs/reference/sbom-format-mapping.md`. The PURL surfaces via the existing A1 row ("PURL") which already has full CDX/SPDX 2.3/SPDX 3 extractor coverage. The parity test harness exercises A1 against every emitted component automatically; alpm components ride through that existing coverage.

The existing `mikebom:source-type` (C1), `mikebom:evidence-kind` (C4), `mikebom:sbom-tier` (C5) annotations emitted on every alpm component reuse those existing rows — alpm contributes new VALUES (`"alpm"`, `"alpm-local-db"`, `"deployed"`) to those rows' value sets but does not alter their wire shape.
