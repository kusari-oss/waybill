# Contract — `pkg:brew/*` component PURL

The only wire-format contract this feature introduces. Per Constitution Principle V audit (research §R1), no `mikebom:*` annotation is introduced — the PURL itself IS the component identity, riding on the standards-native CDX `components[].purl` / SPDX 2.3 `externalRefs[purl]` / SPDX 3 `software_packageUrl` carriers.

**Note on type-name status**: the `brew` PURL type is NOT yet defined in the [purl-spec PURL-TYPES.rst](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst). mikebom emits `pkg:brew/...` per de-facto industry convention (used by syft, cyclonedx-bom-gen, others). A follow-up issue should propose formal registration.

## Wire shape

### Formula

```text
pkg:brew/<formula-name>@<version>[?tap=<owner>/<tap-name>]
```

### Cask

```text
pkg:brew/<cask-token>@<version>?type=cask[&tap=<owner>/<tap-name>]
```

### Segments

| Segment | Source | Encoding | Examples |
|---|---|---|---|
| `pkg:brew/` | Fixed | Verbatim | `pkg:brew/` |
| `<formula-name>` / `<cask-token>` | Cellar / Caskroom directory name | Percent-encoded per PURL spec | `curl`, `openssl@3`, `python@3.12`, `lib32-…` n/a (no lib32 convention in Homebrew) |
| `@<version>` | Cellar / Caskroom subdirectory name | Percent-encoded per PURL spec | `@8.5.0`, `@3.2.1_1`, `@1.95.3` |
| `?type=cask` | Discriminator for cask emissions (FR-005) — always present on cask PURLs, always ABSENT on formula PURLs | Verbatim | `?type=cask` |
| `?tap=` or `&tap=` | `source.tap` from `INSTALL_RECEIPT.json` (formulae) / cask's source tap (casks) — present when non-default; OMITTED when tap is `"homebrew/core"` (formulae) or `"homebrew/cask"` (casks); OMITTED when tap is `null` or absent | Percent-encoded per PURL spec | `?tap=hashicorp/tap`, `&tap=mongodb/brew` |

### Qualifier ordering

When multiple qualifiers are present, they MUST appear in sorted-key order per purl-spec (`tap` < `type` alphabetically):

```text
pkg:brew/firefox@121.0?tap=homebrew/cask-mongoless&type=cask
```

## Examples

| Scan target | Emitted PURL |
|---|---|
| Apple Silicon `/opt/homebrew/Cellar/curl/8.5.0/` with `source.tap = "homebrew/core"` | `pkg:brew/curl@8.5.0` |
| Same install with no `source.tap` (raw-path install) | `pkg:brew/curl@8.5.0` (treated as default; qualifier omitted) |
| Intel macOS `/usr/local/Cellar/openssl@3/3.4.0/` | `pkg:brew/openssl@3@3.4.0` |
| Linuxbrew `/home/linuxbrew/.linuxbrew/Cellar/python@3.12/3.12.7/` | `pkg:brew/python@3.12@3.12.7` |
| Third-party tap: `/opt/homebrew/Cellar/terraform/1.10.0/` with `source.tap = "hashicorp/tap"` | `pkg:brew/terraform@1.10.0?tap=hashicorp/tap` |
| Cask: `/opt/homebrew/Caskroom/visual-studio-code/1.95.3/` | `pkg:brew/visual-studio-code@1.95.3?type=cask` |
| Cask from non-default tap: `/opt/homebrew/Caskroom/intellij-idea/2024.3/` with `source.tap = "homebrew/cask-versions"` | `pkg:brew/intellij-idea@2024.3?tap=homebrew/cask-versions&type=cask` |

## Per-format emission

### CycloneDX 1.6

Location: `.components[].purl` (native).

```json
{
  "type": "library",
  "name": "curl",
  "version": "8.5.0",
  "purl": "pkg:brew/curl@8.5.0",
  "properties": [
    { "name": "mikebom:source-type", "value": "brew" },
    { "name": "mikebom:evidence-kind", "value": "brew-install-receipt" },
    { "name": "mikebom:sbom-tier", "value": "deployed" }
  ]
}
```

The `mikebom:source-type` / `mikebom:evidence-kind` / `mikebom:sbom-tier` properties shown above are the existing per-component annotations emitted by every package-DB reader (milestones 002 / 004 / 002 respectively). The PURL is the only new wire-format addition.

For casks, replace `evidence-kind` value with `"brew-cask-metadata"`.

### SPDX 2.3

Location: `.packages[].externalRefs[]` with `referenceCategory: PACKAGE-MANAGER`.

```json
{
  "name": "curl",
  "versionInfo": "8.5.0",
  "externalRefs": [
    {
      "referenceCategory": "PACKAGE-MANAGER",
      "referenceType": "purl",
      "referenceLocator": "pkg:brew/curl@8.5.0"
    }
  ],
  "annotations": [
    { /* mikebom:source-type / evidence-kind / sbom-tier envelope */ }
  ]
}
```

### SPDX 3.0.1

Location: `software_Package.software_packageUrl` + `Element.externalIdentifier[]` with `externalIdentifierType: "packageUrl"`.

```json
{
  "type": "software_Package",
  "spdxId": "...",
  "name": "curl",
  "software_packageVersion": "8.5.0",
  "software_packageUrl": "pkg:brew/curl@8.5.0",
  "externalIdentifier": [
    {
      "type": "ExternalIdentifier",
      "externalIdentifierType": "packageUrl",
      "identifier": "pkg:brew/curl@8.5.0"
    }
  ]
}
```

## Determinism

For a given on-disk Homebrew install and `INSTALL_RECEIPT.json` content, the emitted PURL set MUST be identical across runs:

- Formula directories are processed in sorted-by-name order (lex-ascending) per the standard `std::fs::read_dir` + sort discipline used by alpm.
- Cask directories are processed in sorted-by-token order.
- Within a formula's runtime_dependencies, order follows the receipt's array order (which is also typically lex-sorted by Homebrew when writing).

## Absence semantics

When the scanned rootfs contains no Homebrew install (none of `/opt/homebrew/Cellar/`, `/usr/local/Cellar/`, `/home/linuxbrew/.linuxbrew/Cellar/` exist):

- Zero `pkg:brew/*` components emit.
- No warnings fire (per FR-006).
- The SBOM document's serialized bytes are identical (modulo timestamps + serial numbers) to a pre-feature scan of the same rootfs (SC-004 invariant).

## Parity-catalog note

Because the wire-format addition is a native PURL (not a `mikebom:*` annotation), no new C-row is added to `docs/reference/sbom-format-mapping.md`. The PURL surfaces via the existing A1 row ("PURL") which already has full CDX/SPDX 2.3/SPDX 3 extractor coverage. The parity test harness exercises A1 against every emitted component automatically; brew components ride through that existing coverage.

The existing `mikebom:source-type` (C1), `mikebom:evidence-kind` (C4), `mikebom:sbom-tier` (C5) annotations emitted on every brew component reuse those existing rows — brew contributes new VALUES (`"brew"`, `"brew-install-receipt"` / `"brew-cask-metadata"`, `"deployed"`) to those rows' value sets but does not alter their wire shape.

## Type-token follow-up

The `brew` type-name is unblessed in the purl-spec. A sibling issue (to file post-merge of this milestone) should:

1. Propose addition of `brew` (or `homebrew`) to [`purl-spec/PURL-TYPES.rst`](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst).
2. Document the canonical shape mikebom + syft + cyclonedx-bom-gen converge on.
3. Once accepted, mikebom's emitted output stays unchanged — only the consumer-side understanding of the type-name shifts from "informal" to "spec-blessed".

Until then, downstream consumers of mikebom output that need to filter on Homebrew components should string-match on `purl.startswith("pkg:brew/")` — same filter shape that works for every other purl-spec-blessed type.
