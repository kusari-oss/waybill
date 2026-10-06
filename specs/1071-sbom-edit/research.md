# Research: Edit an emitted SBOM

Measurements are in `measurements/` (`derivation_probe.py` and its output). `W/` = `waybill-cli/src/`.

## R1 — Native "derived from" links, measured against the validators

**Measured** (`measurements/derivation_probe.txt`). Each probe adds the link to a waybill golden and validates it. Each has a negative control that the validator rejects, so the passes are meaningful.

| format | link | result |
|---|---|---|
| CycloneDX 1.6 | root `externalReferences[{type:"bom", url, hashes:[{alg:"SHA-256"}], comment}]` | accepted (control `type:"derived-from"` rejected) |
| SPDX 2.3 | `externalDocumentRefs[{DocumentRef-original, checksum SHA256}]` + `SPDXRef-DOCUMENT AMENDS DocumentRef-original:SPDXRef-DOCUMENT` | accepted (control `DERIVED_FROM` rejected) |
| SPDX 3.0.1 | `SpdxDocument.import[ExternalMap{verifiedUsing sha256}]` + `Relationship{from: original, amendedBy, to: [this document]}` | accepted by `spdx3-validate` 0.0.5 (control `derivedFrom` rejected) |

**Decision.** Use these three: `AMENDS` / `amendedBy` match SPDX's own meaning ("amends the SPDX information in"), and CycloneDX's `bom` reference is how milestone 072 already points at another BOM. `descendantOf` was also accepted, but "descendant" is SPDX's term for software lineage, not document revision.

**Found in passing, filed as #1147.** Milestone 072 emits `BUILT_FROM` (SPDX 2.3) and `built_from` (SPDX 3), and **both validators reject them**. That's out of scope here. This feature's links are new and validated.

## R2 — Edit model: format-native JSON, not a neutral model

**Measured.** waybill writes CycloneDX and SPDX 3 with sorted keys and 2-space indentation: they re-serialise byte-identically. **Corrected at implementation:** SPDX 2.3 is written in struct field order (`spdxVersion` first), not sorted, so an edited SPDX 2.3 document is the same JSON with its keys sorted. `T/sbom_edit_filter.rs::documents_round_trip` pins both behaviours.

**Decision.**
- Each format has an adapter that edits the document's own JSON in place. Untouched content is byte-identical for waybill-produced documents. For third-party documents it's equal as JSON (key order may change), which is the reading of FR-002 for them.
- Cross-format agreement (FR-013) is enforced by testing: the existing parity extractors run on the three edited outputs of one scan.

**Rejected.**
- **Parse into `ResolvedComponent` and re-emit.** Agreement would hold by construction, but it needs three full parsers, loses anything waybill doesn't model, rewrites every byte, and can't edit third-party documents faithfully.
- **bomctl's protobom-style neutral model.** It is deliberately limited to the NTIA minimum fields, which is the loss waybill exists to avoid.

## R3 — Selecting components (FR-003), per format

For each selector, the fields read:

| selector | CycloneDX | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| PURL pattern, ecosystem | `purl` | `externalRefs[purl]` | `software_packageUrl` |
| lifecycle scope | `waybill:lifecycle-scope` (C42), else `scope:excluded` | `waybill:lifecycle-scope` annotation, else `*_DEPENDENCY_OF` relationship types | `LifecycleScopedRelationship.scope` on edges into the package |
| tier | `waybill:sbom-tier` | annotation | annotation |
| role | `type` + `waybill:component-role` | `primaryPackagePurpose` + annotation | `software_primaryPurpose` + annotation |
| name pattern | `name` | `name` | `name` |

**Decision.**
- The `waybill:` annotation is read first; it exists in all three formats for waybill documents (catalogue rows).
- Native fields are the fallback, which is what makes selection work on third-party documents.
- Patterns are globs (`*`). Name patterns may also be regular expressions, prefixed `re:`.
- Several selectors in one operation combine with AND (FR-003).

## R4 — Dropping and bridging (FR-004, FR-005, FR-006)

**Decision.**
1. Collect the dropped components' identifiers per format: `bom-ref` for CycloneDX, `SPDXID` for SPDX 2.3, `spdxId` for SPDX 3.
2. **Bridging.** For each dropped `B`, every dependent `A` gains an edge to each of `B`'s dependencies `C`, unless one already exists or `A == C`. This repeats until no dropped component remains in any path.
   - **Scope of the bridged edge:** if both edges have the same scope, use it. If one is runtime and the other isn't, the non-runtime one wins: a dependency reached through a dev-only link is dev-only. If they are different non-runtime scopes, the edge into `B` wins, since it is what made `C` reachable from `A`.
   - **Per format:** CycloneDX `dependencies[]`. SPDX 2.3 `DEPENDS_ON` and the reversed `*_DEPENDENCY_OF` types, kept in the direction their type requires. SPDX 3 `dependsOn` (grouped per #1069) and `LifecycleScopedRelationship`.
3. **References removed:**
   - **CycloneDX:** `components[]` (nested too); `dependencies[]` entries and `dependsOn` members; `compositions[].assemblies` and `dependencies`; `vulnerabilities[].affects`, with the vulnerability removed if `affects` empties; `metadata.component` refusal (FR-016).
   - **SPDX 2.3:** `packages[]`, `files[]`, relationships naming it, `hasExtractedLicensingInfos` entries no longer referenced, annotations whose subject is it.
   - **SPDX 3:** the element, relationships whose `from` or `to` holds it (with `to` trimmed, or the relationship removed when it empties), `LicenseExpression` and `Annotation` elements left unreferenced, and VEX/vulnerability elements whose subject is gone.
4. **Completeness (FR-006).** Every component that gained or lost a dependency:
   - **CycloneDX:** moves out of any `compositions[aggregate=complete]` into an `incomplete` record.
   - **SPDX 3:** its `dependsOn` relationship's `completeness` becomes `incomplete`.
   - **SPDX 2.3** has no native construct. The `waybill:graph-completeness` document annotation (C104/C105) is set to incomplete for the affected ecosystems in all three formats, so they stay in agreement.
5. **Post-condition check (runtime, structural).** No identifier of a dropped component remains anywhere in the output (checked by search), and every reference resolves to an element that exists. If either fails, the command fails without writing (Principle IX). Schema and `spdx3-validate` conformance are test-time gates (analysis H1). waybill ships no runtime validator, and Principle I keeps `jsonschema` and the Python validator out of the binary.

## R5 — Redaction (FR-008)

**Decision.**
1. **Collect values per field class** from the fields where that class lives, using the parity catalogue as the field inventory:
   - **paths:** evidence and occurrence locations, `waybill:source-files`, file-tier component names, SPDX `files[].fileName`, `packageFileName`;
   - **URLs and hosts:** external references, download locations, VCS and source-info URLs, identifier annotations, matched against the operator's host pattern;
   - **names:** component `name`, and the PURL name segment, matched against the name pattern.
2. **Replace everywhere.** Each collected value is replaced in every string in the document, not only where it was collected: it also appears in PURLs, `bom-ref`s, annotation payloads and relationship comments.
   - **Boundaries per class:** paths as whole values or as prefixes of longer paths; hosts within URL authorities; names as PURL segments and whole name fields.
   - **PURLs are rewritten as PURLs:** the name segment is replaced and the whole PURL re-encoded, so it stays valid.
3. **Modes.**
   - **remove:** an optional field is deleted. A required field gets a fixed marker. For names, so two components can't collapse into one identity, the marker is `redacted-<n>`, an ordinal within the document.
   - **pseudonymise:** `redacted-` followed by the lowercase base32 of the first 10 bytes of HMAC-SHA256(key, class ‖ 0x00 ‖ value). It's stable across documents for the same key. Different values give different pseudonyms, with collision probability around 2⁻⁸⁰. It can't be reversed or tested without the key. The key is read from `--redact-key-file`, never logged and never written.
4. **Post-condition, fail closed.** After redaction, the serialised output is searched for every collected original value. If any remains, the command fails and writes nothing. A redaction that leaks is worse than none.

**Dependency.** `hmac 0.12.1` is already in `Cargo.lock` (pulled in by sigstore). It is promoted to a direct dependency, adding nothing to the lockfile, as milestone 075 did with `url`.

## R6 — Removing annotations by namespace (FR-007)

**Decision.**
- **CycloneDX:** `properties[]` entries whose `name` starts with the namespace, at document and component level.
- **SPDX 2.3:** `annotations[]` whose comment is a `waybill-annotation/v1` envelope with a matching `field`.
- **SPDX 3:** `Annotation` elements whose statement envelope matches.

The envelope parsing reuses the parity extractors' helpers. Annotations in other namespaces, and anything not in the envelope, are untouched.

**The protected set is always kept** (analysis C1). A namespace match excludes `waybill:generation-context` (C21: the constitution's Operating Modes require every document to state which mode produced it, and removing it would make an edit able to unlabel a document) and `waybill:derivation` (C194). The set is defined once, in `W/edit/mod.rs`, and every adapter consults it.

## R7 — The derivation record and the signature chain (FR-009, FR-011, FR-012)

**Decision.**
- **Record.** The native link (R1), plus a document-scope `waybill:derivation` annotation in each format (catalogue row C194). It holds:
  - `schema`: `waybill-derivation/v1`;
  - `original`: `{sha256, format, signature}`, where `signature` is the original's signing material **embedded**:
    - the CycloneDX JSF object;
    - the DSSE envelope or Sigstore bundle read from the original's sidecar (found next to the input by the existing naming, `<file>.sig.json` / `<file>.sig.bundle.json`, or given by `--original-signature`);
    - `{"kind":"none"}` when the original is unsigned;
  - `operations`: a category and a `matched`/`changed` count for each operation, and no values;
  - `ancestors`: the original's own derivation record, if it had one, so the history survives without the earlier files;
  - `tool` and `created`.

  Embedding the signature makes the derivative self-contained: given the original file, its signature can be checked without hunting for its sidecar.

  **Except when the material holds a redacted value** (analysis H2). A keyless certificate's SAN can name an internal repository, workflow or account, and the material can't be altered without breaking it. Before embedding, the material is searched for every value this edit redacts. On a hit, the record holds `{kind, material_sha256, embedded: false, reason: "contains-redacted-values"}` instead, and `verify-chain` then needs the original's signature file via `--original-signature`, checked against that digest. The leak post-condition (R5) therefore never sees the material.
- **Signing.** The output is signed with the same options as generation (`--sign-key` and keyless `--sign`), through the existing signer. The CycloneDX `signature` slot is stripped and re-signed (`strip_existing_signature`). Sidecars are written for SPDX.
- **Verification**, `waybill sbom verify-chain <derived> [--original <file>]... [--key <pem>]...`:
  - **the derivative's own signature:**
    - static-key JSF or DSSE, natively, with the existing `CosignVerificationKey` path used in `W/sbom/signer.rs` tests and `W/attestation/verifier.rs`;
    - a keyless bundle: waybill checks that the bundle's artifact digest equals the document's hash, and prints the exact `cosign verify-blob` command for the certificate and transparency-log check. Waybill doesn't vendor Fulcio's root or Rekor's key (only CT keys, `W/attestation/sigstore_trust_root.rs`), and m779 already defers to cosign. The report labels this check **delegated**, never **verified**;
  - **each link:** the hash of the supplied original equals `original.sha256`, and the original's embedded signature verifies against the original's bytes;
  - **chains:** `--original` is given once per step, newest first, and each step is checked the same way;
  - **reporting:** the report lists every check as verified, delegated or unavailable (original not supplied), and exits non-zero on any failure.

**Rejected.** Referencing the original's sidecar by path only. A path means nothing to a recipient, and the material is a few KB.

## R8 — Command shape and a policy-ready operation model (FR-001, FR-014, FR-016)

**Decision.** The command is `waybill sbom edit <INPUT> -o <OUTPUT>`. Operations:
- `--drop <selector>`
- `--drop-annotations <namespace>`
- `--redact <class>[:<mode>][=<pattern>]`, with `--redact-key-file <path>`

Signing options are the same as `sbom scan`.

Each flag parses into one `EditOp { action, selector, params }`, a `serde`-deserialisable struct. A policy file in the next milestone is then a list of the same structs, with no new semantics.

Operations apply in order. The command reports `matched` / `changed` per operation, and exits non-zero on an invalid operation, an unsupported input, a refused drop (the root) or a failed post-condition, without writing.

## R9 — Performance baseline (SC-006)

**Not yet measured.** It is an implementation task. The workload is the largest corpus SBOM (`image-postgres16`, 2.1 MB CycloneDX) and the same scan's SPDX outputs. The baseline is the time to read and re-serialise each document unchanged. The SC-006 target is a ratio to the time to generate that document. Both numbers are recorded in `measurements/`.

## R10 — Detecting the input format

**Decision.** Reuse the detection in `W/binding/verify.rs`: `@graph` means SPDX 3, `spdxVersion` and `packages` mean SPDX 2.3, `bomFormat: CycloneDX` means CycloneDX. Anything else, and CycloneDX `specVersion` other than 1.6, is refused (FR-016). The output keeps the input's format and version.

## Implementation notes (2026-10-06)

Decisions made while implementing, recorded against the research item they refine.

- **R3: `optional` scope.** Selectors accept `scope=optional`, waybill's fifth `LifecycleScope`. SPDX 3 has no optional scope and carries it only as `waybill:optional-derivation` (m179). The adapters read that annotation in all three formats, so `scope=optional` matches identically; measured on the corpus goldens: python-flask 22/22/22, rust-ripgrep 3/3/3.
- **R4: completeness.** C104 `waybill:graph-completeness` becomes `unknown`, and C105 (its reason) is removed. `partial` needs a reason code, and the closed vocabulary has none for "an edit changed this list".
- **R5: path inventory.** Paths are collected from one list of path-bearing fields, defined once for all formats: C18, C25, C31, C63, C66, C76, C92, C120, C121, C130, C136, plus D2 evidence occurrences. The first implementation read only C18/C92 in SPDX. On image-postgres16, `--drop-annotations waybill: --redact paths` then matched 6859 paths in CycloneDX and 0 in SPDX, whose evidence rides an `evidence.occurrences` annotation that the drop leaves in place.
- **R5: basenames.** A path's basename is replaced only within a file-tier component, whose name is the file name. Applied within any component, it renamed a package's own `bom-ref` (`postgresql-common`), and would have rewritten a `?arch=` PURL qualifier. Component-scoped forms never touch identifier fields. Identifiers change only through everywhere-forms, together with every reference to them.
- **R5: application.** Each operation's everywhere-forms are replaced in one multi-pattern pass, longest first. The first implementation walked the document once per value: 6.0 s on the 2 MB image-postgres16 CycloneDX golden, now 0.12 s (`measurements/performance.txt`).
- **R5: remove mode.** Values become opaque markers, distinct per value: `redacted-path-<n>`, `redacted-host-<n>.invalid`, `redacted-<n>`. Hosts always end in `.invalid` (RFC 6761), pseudonymised or not, so they stay valid hostnames.
- **R5: leak check.** The check searches every string and what any base64 string decodes to: certificates and DSSE envelopes are base64.
- **R7: payloads.** Signature material is embedded without its signed payload. A DSSE envelope's payload is the whole original document, and embedding it would undo every drop and redaction; the verifier has the original's bytes anyway. `material_sha256` is computed over the payload-free material.
- **R7: the original's signature.** It is removed from the output: the CycloneDX `signature` and the m778 `attestation` reference to a keyless sidecar. It signs bytes that no longer exist; its material is in the record.
- **R7: document identity.** A derived document is a new document:
  - CycloneDX keeps `serialNumber` and increments `version`, CycloneDX's own model for a modified BOM;
  - SPDX 2.3 gets a new `documentNamespace` (`<original>-edit-<16 hex>`);
  - SPDX 3 gets a new `SpdxDocument` IRI, with references to the document itself updated, while its elements keep their IRIs.

  The CycloneDX record is a `metadata.properties` entry, where document-scope `waybill:` annotations live.
- **R7: verification keys.** Static-key signatures verify only against `--key`. The key a signature embeds is never trusted. Passing any `--key` also requires the derived document to be signed, so a stripped signature fails (tamper case (d)) instead of reading as unsigned.
- **Found while testing, not fixed here:** SPDX 3 loses a component's lifecycle scope when the component has no incoming dependency edge. The scope rides only on the relationship, and no `waybill:lifecycle-scope` annotation is emitted. Example: maven-guice `guice-testlib` is test-scoped in CycloneDX and SPDX 2.3 and unscoped in SPDX 3, so `--drop scope=development,test` matches 17/17/16 there. This is an emitter parity gap, filed as #1148.

