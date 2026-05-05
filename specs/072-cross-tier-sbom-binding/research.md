# Research — milestone 072 cross-tier SBOM binding

## Decision summary

| Decision | Choice | Section |
|---|---|---|
| D1 — Binding hash algorithm | SHA-256 over canonical JSON envelope `{vcs, lockfile, manifest, algo:v1}` | §1 (spec Q1) |
| D2 — Per-ecosystem input sources | Reuse existing extraction sites (`GoVcsInfo`, cargo workspace context, etc.); per-ecosystem mapping table | §1 |
| D3 — Standards-native cross-document refs | CDX `externalReferences[type:bom]` + SPDX `externalDocumentRefs` + `BUILT_FROM`/`GENERATED_FROM` relationship | §3 |
| D4 — Per-instance VEX carrier | Existing OpenVEX 0.2.0 `Statement.products[].identifiers` map (no upstream schema fork) | §2 (spec Q2) |
| D5 — VEX propagation mode CLI placement | Flag on `mikebom sbom enrich` (`--vex-propagation-mode`, default `caveated`) | §4 (spec Q3) |
| D6 — JSON canonicalization primitive | Reuse milestone-071 `canonicalize_for_compare` helper | §5 |

---

## §1 — Per-ecosystem input source extraction

For each of mikebom's 6 source-tier ecosystems, the table below lists the canonical `(vcs, lockfile, manifest)` triple the FR-002 binding hash consumes and the existing mikebom code site each input is already extracted from.

| Ecosystem | VCS commit source | Lockfile (SHA-256 input) | Manifest (SHA-256 input) | Extraction sites |
|---|---|---|---|---|
| **golang** | Go BuildInfo `vcs.revision` (binary-tier); `git rev-parse HEAD` (source-tier) | `go.sum` | `go.mod` | `scan_fs/package_db/go_binary.rs:66+` (`GoVcsInfo`); `scan_fs/package_db/golang/legacy.rs` for go.sum/go.mod walking |
| **cargo** | `cargo-auditable` embedded VCS (binary-tier); `git rev-parse HEAD` from project root (source-tier) | `Cargo.lock` | top-level `Cargo.toml` | `scan_fs/package_db/cargo.rs:344+` (`build_cargo_main_module_entry`, `discover_workspace_manifests`) |
| **npm** | `git rev-parse HEAD` (source-tier only — no widespread binary-embed convention) | `package-lock.json` (or `yarn.lock` / `pnpm-lock.yaml` fallback) | top-level `package.json` | `scan_fs/package_db/npm/walk.rs` + `npm/mod.rs` (milestone 066 main-module emission) |
| **pip** | `git rev-parse HEAD` (source-tier) | `poetry.lock` (Poetry projects); `requirements*.txt --hash=` (PEP 503) ; `pdm.lock` (PDM); SHA-256 of the lockfile bytes | top-level `pyproject.toml` (PEP 621) | `scan_fs/package_db/pip/mod.rs` (milestone 068 main-module emission) |
| **gem** | `git rev-parse HEAD` (source-tier) | `Gemfile.lock` | top-level `*.gemspec` | `scan_fs/package_db/gem.rs` (milestone 069 main-module emission) |
| **maven** | `git rev-parse HEAD` (source-tier); future: `<scm>` block in pom.xml | (none — Maven has no canonical lockfile in the milestone-070 emission pattern; manifest-only binding for maven, strength capped at `weak` unless content-hash sidecar lands later) | top-level `pom.xml` (after parent inheritance + property substitution) | `scan_fs/package_db/maven.rs` (milestone 070 main-module emission) |

**Per-ecosystem strength rules** (per FR-012):

- `verified` — both `vcs` AND `lockfile` AND `manifest` populated AND match. Cargo / npm / pip / gem source-tier scans of git checkouts hit this naturally.
- `weak` — exactly two of the three populated AND match. Common when the source scan ran outside a git checkout (no VCS) or in maven (no lockfile).
- `unknown` — fewer than two populated, or any side fails to match the source-tier recomputation.

**Extraction code reuse rule**: the new `binding/source_inputs.rs` module dispatches per-ecosystem to existing extractors. Don't re-walk the source tree; consume what `scan_fs/package_db/{ecosystem}.rs` already produced (the `PackageDbEntry` for the main-module already carries the necessary file paths in `.evidence.source_file_paths` and the relevant lockfile path).

**Rationale**: Reuses the milestone 053–070 main-module discovery work as the input substrate. Adding binding emission doesn't require any new walker code; it just hashes inputs the existing walkers already located.

**Alternatives considered**:

- *Hash the entire source tree (Merkle of all files)*. Rejected — too expensive on large repos, brittle to whitespace/file-ordering changes, doesn't capture the user's intent (we want commit-level identity, not byte-level identity).
- *Sign the source SBOM and verify the signature instead of recomputing the hash*. Rejected — adds key-management dependencies; orthogonal to binding identity (signing is provenance, binding is identity).
- *Use just the source SBOM document SHA-256 as the binding*. Rejected — that's Option A in spec Q1, doesn't survive the user's "different foo built from same inputs at different times" case.

---

## §2 — OpenVEX 0.2.0 `Product.identifiers` schema

**Decision**: Extend the existing `OpenVexProduct` struct at `mikebom-cli/src/generate/openvex/statements.rs:71` with `pub identifiers: BTreeMap<String, String>` (using `serde(skip_serializing_if = "BTreeMap::is_empty")` so pre-072 emission shape is preserved when no instance identifiers are needed).

**OpenVEX 0.2.0 wire shape** (from the published `openvex.dev` spec):

```json
{
  "@id": "pkg:golang/golang.org/x/net@v0.28.0",
  "identifiers": {
    "purl": "pkg:golang/golang.org/x/net@v0.28.0",
    "cyclonedx-bom-ref": "pkg:golang/golang.org/x/net@v0.28.0?bomref=imageinstance-3",
    "spdx-spdxid": "SPDXRef-imageinstance-3-net"
  },
  "subcomponents": []
}
```

The `identifiers` map is part of OpenVEX 0.2.0 (`Product.identifiers: { [identifier_type]: string }`) and is currently underutilized. Filling it with mikebom's per-instance bom-ref / SPDXID is wire-compatible: pre-072 OpenVEX consumers (`vexctl`, etc.) match by `@id` and ignore unfamiliar identifier-type keys; post-072-aware consumers (mikebom-built tools, future `vexctl` versions) can disambiguate per-instance.

**Rationale**:

- Doesn't extend the OpenVEX schema — uses an existing field.
- The user's clarification (Q2) explicitly chose this hybrid path.
- Pre-072 fallback is automatic: a consumer doing `for product in statement.products` matching by `@id` still gets a correct per-PURL view, just at coarser granularity.
- Per-instance support is opt-in for downstream tools; doesn't force them to upgrade.

**Alternatives considered**:

- *Extend OpenVEX with a `mikebom_bom_ref` proprietary field*. Rejected — Constitution Principle V's standards-native preference; the existing `identifiers` map already serves the use case.
- *Use OpenVEX `subcomponents` array instead of `identifiers`*. Rejected — `subcomponents` is for "this CVE applies to X but only when X is vendored inside Y", not "this CVE applies to instance Z of X". Wrong semantic.

---

## §3 — Cross-document-reference shapes per format

**Decision**: Standards-native cross-document references attach at the **document level** in all three formats (per FR-004), with mikebom's `mikebom:source-document-binding` annotation attaching at the **component level** for the per-component hash + strength.

### CDX 1.6

`externalReferences[]` at the document level (`metadata.component` plus optionally on the document itself):

```json
{
  "metadata": {
    "component": {
      "name": "image:foo:v1.0",
      "type": "container",
      "externalReferences": [
        {
          "type": "bom",
          "url": "https://example.org/sbom/foo-v1.0-source.cdx.json",
          "comment": "source-tier SBOM for the binary foo built from this image",
          "hashes": [
            { "alg": "SHA-256", "content": "<sha256-of-source-sbom-bytes>" }
          ]
        }
      ]
    }
  }
}
```

`externalReferences[].type: "bom"` is the CDX 1.6 native cross-document reference type — exactly the semantic mikebom needs.

### SPDX 2.3

Two-part construct:

1. **Document-level** `externalDocumentRefs[]` array names the source SBOM:

   ```json
   "externalDocumentRefs": [
     {
       "externalDocumentId": "DocumentRef-source-foo-v1.0",
       "spdxDocument": "https://example.org/sbom/foo-v1.0-source.spdx.json",
       "checksum": {
         "algorithm": "SHA256",
         "checksumValue": "<sha256-of-source-sbom-bytes>"
       }
     }
   ]
   ```

2. **Per-component-level** `relationships[]` array binds via SPDX 2.3 native relationship types:

   ```json
   "relationships": [
     {
       "spdxElementId": "SPDXRef-foo-image-binary",
       "relatedSpdxElement": "DocumentRef-source-foo-v1.0:SPDXRef-foo-source-package",
       "relationshipType": "BUILT_FROM"
     }
   ]
   ```

`BUILT_FROM` (SPDX 2.3 §11.1) is the native semantic; `GENERATED_FROM` (also §11.1) is an acceptable synonym for some cases (e.g., generated source code bound to its generator). mikebom emits `BUILT_FROM` for binary-from-source bindings.

### SPDX 3.0.1

`@graph[]` element with `type: "Relationship"` and `relationshipType: "built_from"` (lowercase per SPDX 3 convention):

```json
{
  "type": "Relationship",
  "spdxId": "https://example.org/spdx/rel-built-from-1",
  "from": "https://example.org/spdx/foo-image-binary",
  "to": ["https://example.org/spdx/foo-source-package"],
  "relationshipType": "built_from"
}
```

Cross-document references in SPDX 3 use the `import` field on `SpdxDocument`:

```json
{
  "type": "SpdxDocument",
  "spdxId": "https://example.org/spdx/image-doc",
  "import": [
    {
      "type": "ExternalMap",
      "externalSpdxId": "https://example.org/sbom/foo-v1.0-source.spdx3.json",
      "verifiedUsing": [
        { "type": "Hash", "algorithm": "sha256", "hashValue": "<sha256>" }
      ]
    }
  ]
}
```

**Rationale**: All three format-native constructs are spec-compliant, well-known to existing SBOM tooling (Trivy, syft, sbomqs all decode them today), and carry the document-level identity + per-component binding edge that mikebom needs.

**Alternatives considered**:

- *Single CDX/SPDX dependency edge* (`type: "depends-on"` / `relationshipType: "DEPENDS_ON"`). Rejected — semantically wrong; this isn't a runtime-deps edge, it's a provenance edge.
- *mikebom-only annotation for cross-document refs*. Rejected — Principle V violation; the native fields exist.

---

## §4 — VEX propagation mode wiring into `mikebom sbom enrich`

**Decision**: Add `--vex-propagation-mode {permissive,caveated,strict}` flag to `mikebom-cli/src/cli/enrich.rs` `EnrichArgs` struct. Default value `caveated`. The existing legacy `--vex-overrides <path>` no-op flag becomes the input source for VEX statements to propagate; the flag's behavior is now defined by `--vex-propagation-mode`.

**Implementation surface**:

- `EnrichArgs` gains `vex_propagation_mode: VexPropagationMode` with `#[derive(ValueEnum)]` per the project's existing `ImageSource` pattern (avoiding the milestone-071 lesson learned from the original Mario PR review).
- `cli/enrich.rs::execute` calls into `sbom/mutator.rs::enrich` (existing JSON-Patch path) AND a new `sbom/mutator.rs::propagate_vex_with_binding(args.vex_propagation_mode, source_sbom, target_sbom)` step when `--vex-overrides` is supplied.
- The propagation step reads each VEX statement from `--vex-overrides`, looks up the target component(s) in the target SBOM, checks `mikebom:source-document-binding` for binding strength, and applies per-mode rules:
  - `permissive` — propagate by PURL match, no binding check (pre-072 behavior preserved for back-compat).
  - `caveated` — propagate; if binding strength is not `verified`, add a `binding-unverified` justification override + a structured `mikebom:vex-binding-status` annotation per OpenVEX statement.
  - `strict` — refuse propagation when binding is not `verified`; write a refusal-rationale annotation; exit non-zero.

**Rationale**: The Q3 clarification chose this CLI placement specifically to keep blast radius small. Existing users invoking `mikebom sbom enrich --patch X.json` (with no VEX overrides) see zero behavior change. New users opting into VEX propagation get the safer default automatically.

**Alternatives considered**:

- *New `mikebom sbom vex propagate` subcommand*. Rejected per Q3 — splits CLI surface for one milestone's work.
- *Auto-detect mode from input SBOM annotation*. Rejected per Q3 — conflates emission and consumption concerns.

---

## §5 — JSON canonicalization primitive reuse

**Decision**: Use the milestone-071 `canonicalize_for_compare` helper at `parity/extractors/common.rs:96` for the binding-hash envelope's canonical JSON serialization. The helper already implements: sorted object keys (lex), sorted arrays (lex when `order_sensitive=false`), normalized whitespace via `serde_json::to_string`. Exactly the shape FR-002's "canonical JSON envelope" needs.

**Rationale**: The canonicalization rule is the same one milestone 071 contracted for cross-format value-equality. Using a single canonical-JSON primitive keeps the spec contract internally consistent — the binding-hash canonical form is the SAME canonical form parity comparison uses, so a verifier-side reimplementation that gets the parity canonicalization right will also get the binding-hash canonicalization right (no second algorithm to learn).

**Alternatives considered**:

- *Roll a custom canonical-JSON serializer for binding only*. Rejected — duplication of effort + risk of drift between two canonicalizers.
- *Use JCS (JSON Canonicalization Scheme, RFC 8785)*. Rejected — adds a new crate dependency; existing helper is sufficient.

---

## §6 — Source-tier SBOM document identifier

**Decision**: A `SourceDocumentId` struct with two fields:

- `sha256: String` — SHA-256 hex of the canonical source SBOM bytes (CDX or SPDX); the verifier-side computable identifier.
- `iri: Option<String>` — optional URI/IRI for human-readable cross-reference (e.g., `https://example.org/sbom/foo-v1.0-source.cdx.json` or `urn:uuid:...`).

Both are emitted into the cross-document reference fields per format (§3). A consumer with the source SBOM file in hand can recompute `sha256` and verify it matches; a consumer without can use `iri` to fetch.

**Rationale**: SHA-256 is the cryptographically meaningful identifier (tamper-evident); IRI is the human-friendly handle. Both are supported by all three target formats' cross-document reference shapes.

**Alternatives considered**:

- *DSSE envelope of the source SBOM*. Rejected — adds signing infrastructure; the binding hash already provides tamper-evident identity for the inputs that matter (vcs/lockfile/manifest), and signing the SBOM document is orthogonal (Principle X: layer in-toto attestations on top if needed).

---

## §7 — Existing per-instance bom-ref / SPDXID emission

**Decision**: No new emission code. Mikebom's existing CDX builder already emits unique `bom-ref` per component instance; the SPDX 2.3 emitter assigns unique `SPDXID` per Package; the SPDX 3 emitter assigns unique `spdxId` per Package element. The milestone-071 holistic_parity test verifies these are stable across the three formats.

**Rationale**: Per-instance VEX (FR-008) needs each format's per-instance identifier to be stable and round-trippable. Mikebom already meets this — verified by the existing test infrastructure. The work in this milestone is on the OpenVEX-emit side, not on per-format component identification.

---

## Open items deferred to Phase 1

None. All architectural choices are pinned; the data-model.md and contracts/ artifacts capture the concrete shapes; quickstart.md captures operator-visible behavior.
