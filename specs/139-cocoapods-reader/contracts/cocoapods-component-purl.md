# Contract — `pkg:cocoapods/*` and `pkg:generic/*` component PURL

The only wire-format contract this feature introduces. Per Constitution Principle V audit (research §R1):

- `pkg:cocoapods/` is **purl-spec-blessed** ([cocoapods-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md)) — used for trunk + git source pods, with the `#subpath` mechanism for subspecs.
- `pkg:generic/` is the placeholder for path source pods (purl-spec does not define a `cocoapods-path` type).
- The source-type discriminator surfaces via the existing parity-catalog C1 row (`mikebom:source-type` annotation) — no new C-row added; CocoaPods contributes new VALUES (`cocoapods-trunk` / `cocoapods-git` / `cocoapods-path` / `cocoapods-main-module`) to C1's value set.

## Wire shapes per source

### Trunk (default CDN)

```text
pkg:cocoapods/<pod>@<version>
```

Pod name case-preserved verbatim (CocoaPods is case-sensitive per purl-spec). Version verbatim from `Podfile.lock`'s parenthesized form (parentheses stripped).

### Trunk subspec

```text
pkg:cocoapods/<root-pod>@<version>#<subpath>
```

Per Phase 0 research correction: subspecs encode via the PURL `#subpath` mechanism (NOT a `?subspec=` qualifier — that was the initial spec guess). Examples:
- `Firebase/Core 10.20.0` → `pkg:cocoapods/Firebase@10.20.0#Core`
- `Firebase/Database/Realtime 10.20.0` → `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (raw `/` between subpath segments; only RFC-3986-unsafe characters within segments are percent-encoded)
- `GoogleUtilities/Environment 7.5.2` → `pkg:cocoapods/GoogleUtilities@7.5.2#Environment`

### Git source

```text
pkg:cocoapods/<pod>@<version>?vcs_url=git+<git-remote-url>
```

URL from `EXTERNAL SOURCES{pod}{:git}`. Resolved 40-char SHA from `CHECKOUT OPTIONS{pod}{:commit}` flows into `mikebom:vcs-ref` annotation per Q2 (NOT embedded in PURL version — the lockfile's `(version)` field IS the upstream identity for git-source pods).

### Path source

```text
pkg:generic/<flattened-pod>@<version>
```

Where `<flattened-pod>` is the pod name with any `/` replaced by `-` (e.g., `Firebase/Core` → `Firebase-Core`). Flattening avoids the `pkg:generic/<namespace>/<name>` ambiguity per purl-spec base rules — `pkg:generic/Firebase/Core@1.0` would parse as `namespace=Firebase, name=Core` which loses the subspec semantics. Matches the milestone-138 composer-reader convention (`pkg:generic/<vendor>-<package>`).

Plus `mikebom:source-type = "cocoapods-path"` annotation + `mikebom:path = "<EXTERNAL-SOURCES-path-value>"` annotation. For path-sourced subspecs, also include `mikebom:subspec = "<original-subspec-path>"` annotation so consumers can recover the original `<pod>/<subspec>` form. Path-deps have no trunk-addressable identity.

### Main-module (per FR-012 + Q1 cascade)

```text
pkg:cocoapods/<app-name>@0.0.0-unknown
```

Plus `mikebom:component-role = "main-module"` + `mikebom:source-type = "cocoapods-main-module"` annotations. App-name derivation cascade per Q1:
1. First `target '<name>' do` block in `Podfile`.
2. Parent-directory basename fallback when no Podfile exists OR Podfile lacks any parseable target block.

## Examples

| Scan input | Emitted PURL |
|---|---|
| `Podfile` target `'MyApp'` + Podfile.lock | `pkg:cocoapods/MyApp@0.0.0-unknown` (main-module) |
| Podfile.lock PODS entry `AFNetworking (4.0.1)` + SPEC CHECKSUMS `AFNetworking: abc...` | `pkg:cocoapods/AFNetworking@4.0.1` (hashes: `[sha1:abc...]`) |
| Podfile.lock PODS entry `Firebase/Core (10.20.0)` + SPEC CHECKSUMS `Firebase: def...` | `pkg:cocoapods/Firebase@10.20.0#Core` (hashes: `[sha1:def...]` — root-keyed) |
| Podfile.lock PODS entry `Firebase/Auth (10.20.0)` (same fixture) | `pkg:cocoapods/Firebase@10.20.0#Auth` (hashes: `[sha1:def...]` — same as Core) |
| Podfile.lock PODS entry `GoogleUtilities/Environment (7.5.2)` | `pkg:cocoapods/GoogleUtilities@7.5.2#Environment` |
| EXTERNAL SOURCES `MyFork: {:git: 'https://github.com/foo/my-fork.git', :branch: 'main'}` + CHECKOUT OPTIONS `MyFork: {:commit: 'eb39649...'}`, PODS `MyFork (1.5.0)` | `pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git` (annotations: `mikebom:vcs-ref = "eb39649..."`, `mikebom:vcs-declared-ref = "main"`) |
| EXTERNAL SOURCES `LocalLib: {:path: '../packages/local-lib'}`, PODS `LocalLib (0.1.0)` | `pkg:generic/LocalLib@0.1.0` (annotations: `mikebom:source-type = "cocoapods-path"`, `mikebom:path = "../packages/local-lib"`) |
| EXTERNAL SOURCES `Firebase/Core: {:path: '../firebase-core'}`, PODS `Firebase/Core (10.20.0)` (path-sourced subspec) | `pkg:generic/Firebase-Core@10.20.0` (annotations: `mikebom:source-type = "cocoapods-path"`, `mikebom:path = "../firebase-core"`, `mikebom:subspec = "Core"` for recovery of original subspec form) |
| Lockfile-only commit (no Podfile) in directory `MyContainerApp/` | Main-module: `pkg:cocoapods/MyContainerApp@0.0.0-unknown` (Q1 dir-basename fallback) |
| `Pods/Manifest.lock` only (no sibling Podfile.lock) | All emitted components carry `mikebom:sbom-tier = "deployed"` + `mikebom:evidence-kind = "cocoapods-manifest-lock"` per Q3 |

## Per-format emission

### CycloneDX 1.6

Location: `.components[].purl` (native).

```json
{
  "type": "library",
  "name": "Firebase/Core",
  "version": "10.20.0",
  "purl": "pkg:cocoapods/Firebase@10.20.0#Core",
  "hashes": [
    {"alg": "SHA-1", "content": "<lowercase-hex-from-SPEC-CHECKSUMS-by-root-key-Firebase>"}
  ],
  "properties": [
    {"name": "mikebom:source-type", "value": "cocoapods-trunk"},
    {"name": "mikebom:evidence-kind", "value": "cocoapods-podfile-lock"},
    {"name": "mikebom:sbom-tier", "value": "source"}
  ]
}
```

The `mikebom:source-type` / `mikebom:evidence-kind` / `mikebom:sbom-tier` properties are existing per-component annotations. The PURL (with `#subpath` for subspecs) + SHA-1 hash + `mikebom:vcs-ref` / `mikebom:path` / `mikebom:vcs-declared-ref` (source-type-specific) are the only new wire-format additions.

### SPDX 2.3

Location: `.packages[].externalRefs[]` with `referenceCategory: PACKAGE-MANAGER`.

```json
{
  "name": "Firebase/Core",
  "versionInfo": "10.20.0",
  "externalRefs": [
    {
      "referenceCategory": "PACKAGE-MANAGER",
      "referenceType": "purl",
      "referenceLocator": "pkg:cocoapods/Firebase@10.20.0#Core"
    }
  ],
  "checksums": [
    {"algorithm": "SHA1", "checksumValue": "<hex>"}
  ]
}
```

### SPDX 3.0.1

Location: `software_Package.software_packageUrl` + `Element.externalIdentifier[]`.

```json
{
  "type": "software_Package",
  "spdxId": "...",
  "name": "Firebase/Core",
  "software_packageVersion": "10.20.0",
  "software_packageUrl": "pkg:cocoapods/Firebase@10.20.0#Core",
  "externalIdentifier": [
    {
      "type": "ExternalIdentifier",
      "externalIdentifierType": "packageUrl",
      "identifier": "pkg:cocoapods/Firebase@10.20.0#Core"
    }
  ]
}
```

## Determinism

For a given `Podfile.lock` / `Podfile` / `Manifest.lock`, the emitted PURL set MUST be identical across runs:

- PODS entries processed in their YAML-array order (preserved by `serde_yaml`).
- Main-module components processed in walker discovery order (sorted directory entries per `safe_walk` convention from milestone 114).
- `extra_annotations` `BTreeMap` ensures deterministic property emission order.

## Absence semantics

When the scanned root contains none of `Podfile.lock` / `Podfile` / `Pods/Manifest.lock`:

- Zero `pkg:cocoapods/*` and zero CocoaPods-derived `pkg:generic/*` components emit.
- No warnings fire (per FR-006).
- SBOM bytes are identical (modulo timestamps + serial numbers) to a pre-feature scan (SC-004 invariant).

## Parity-catalog note

Because the wire-format addition is the native PURL field (including the spec-blessed `#subpath` for subspecs), no new C-row is added to `docs/reference/sbom-format-mapping.md` for identity. The PURL surfaces via the existing A1 row ("PURL").

The `mikebom:source-type` annotation reuses C1; CocoaPods contributes new VALUES (`cocoapods-trunk` / `cocoapods-git` / `cocoapods-path` / `cocoapods-main-module`) to C1's value set without altering wire shape.

The new annotations introduced by this milestone:
- `mikebom:vcs-ref` (resolved git SHA) — reuses milestone-138's annotation name (composer used it identically; same semantics).
- `mikebom:vcs-declared-ref` (operator-declared `:branch`/`:tag`/`:commit` from EXTERNAL SOURCES, when distinct from resolved) — NEW. Deferred parity-catalog refresh.
- `mikebom:path` (path-source path string) — reuses milestone-137 + 138 precedent.
- `mikebom:subspec` (subpath value duplicated as an annotation for easier filtering) — NEW. Deferred parity-catalog refresh; informational redundant with PURL `#subpath`.

## syft/trivy divergence (deferred to v1.1)

Per Phase 0 research, both syft and trivy fold subspec into the pod name (`pkg:cocoapods/Firebase/Database@1.0.0`) instead of using the purl-spec-canonical `#subpath` form. mikebom emits the spec-conformant form per Principle V (standards-native first). Emitting a `mikebom:also-known-as = "pkg:cocoapods/<root>/<subspec>@<version>"` annotation for syft/trivy compatibility is **deferred to v1.1** — operators who need that ecosystem's compatibility can use those tools directly in the interim.
