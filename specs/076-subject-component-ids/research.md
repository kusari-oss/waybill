# Research — milestone 076 subject identifier + per-component identifiers

Six implementation-level decisions to pin before Phase 1 design.

## §1 — CDX 1.6 `externalReferences[].type` enum value for document-level `subject:`

**Decision**: Use `attestation`. Same enum value milestone 073 already uses for the `attestation:` scheme (the IRI form). External tools disambiguate by inspecting the URL value: an IRI shape (`https://...`) means an attestation reference; a `<algo>:<hex>` shape means a subject hash.

**Rationale**: Three reasons stack:
1. **Supply-chain mental model fit.** A subject hash IS what an attestation cryptographically commits to. Calling it `attestation` in the externalReferences type tells consumers "this is part of the supply-chain attestation story" without inventing a new enum value.
2. **No new mikebom-specific subtype hint needed.** Operators reading the SBOM will see `attestation`-typed entries and recognize them as supply-chain-related; the value form (IRI vs digest) is the per-entry disambiguator.
3. **Consistency with 073's existing precedent.** `attestation:` IRIs already ride `attestation`; extending the same type to subject hashes preserves the symmetric mapping that the project's verification harness expects.

The downside — that consumers naively expecting an IRI in every `attestation`-typed entry will see a digest string instead — is the smaller cost compared to inventing a new enum value (which would force every consumer to be updated to recognize the new value) or using `other` (which throws away the supply-chain semantic hint entirely).

**Alternatives considered**:
- `formulation` — semantically about how a component is *built*, which is adjacent but not identical to "this IS the build output." Subjects are outputs, not formulation descriptions.
- `evidence` — too generic; loses the supply-chain-attestation hint.
- `other` — semantic-poor fallback. Forces every consumer to read the comment field to interpret the entry. Loses interop value.
- New CDX enum value — would require a CDX spec PR with cross-vendor consensus before mikebom could use it. Out of scope for this milestone.

**Wire shape consequence**: a build-tier SBOM with one auto-detected subject emits:
```json
{
  "type": "attestation",
  "url": "sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab",
  "comment": "auto-detected from build-tier in-toto subject `myapp`"
}
```

Co-existing with the milestone 073 attestation: IRI form when both are present:
```json
[
  {
    "type": "attestation",
    "url": "https://example.com/myapp-build-001",
    "comment": "manual --attestation"
  },
  {
    "type": "attestation",
    "url": "sha256:abc1234567890...",
    "comment": "auto-detected from build-tier in-toto subject `myapp`"
  }
]
```

## §2 — CDX 1.6 carrier choice for per-component user-defined identifiers

**Decision**: `components[].properties[]` with `name = "<scheme>"` and `value = "<value>"`.

**Rationale**: The CDX 1.6 spec describes `properties` as *"a name-value store … flexibility to include data not officially supported in the standard without having to use additional namespaces"*. That is the literal use case: arbitrary user-defined `(scheme, value)` pairs whose semantics aren't in the CDX type taxonomy. By contrast, `externalReferences[]` is described as *"external references … to systems, sites, and information"* — semantically about pointing OUT to other systems, not about arbitrary local identifiers attached to the component in question.

For Constitution Principle V's native-precedence rule:
- `properties[]` is a native CDX field that exactly matches the semantic ("arbitrary key-value identifier scoped to this component"). Native fit: high.
- `externalReferences[]` would force fitting into a URL/URI-shaped slot (the `url` field is required). Native fit: poor, requires shoe-horning.

Per-component user-defined identifiers fit `properties[]` naturally. Use it.

**Wire shape consequence**: a `--component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2"` flag produces, on the matching component:
```json
{
  "type": "library",
  "name": "serde",
  "version": "1.0.0",
  "purl": "pkg:cargo/serde@1.0.0",
  "properties": [
    {"name": "kusari-id", "value": "asset-shared-lib-v2"}
  ]
}
```

If the operator passes multiple `--component-id` flags matching the same component, each becomes a separate `properties[]` entry, in lexical order by `(name, value)` for stable serialization.

**Symmetric per-format mapping**:
| Format | Carrier |
|--------|---------|
| CDX 1.6 | `components[].properties[]` with `name=<scheme>`, `value=<value>` |
| SPDX 2.3 | `Package.externalRefs[]` with `referenceCategory=PERSISTENT-ID`, `referenceType=<scheme>`, `referenceLocator=<value>` |
| SPDX 3 | `Element.externalIdentifier[]` with `type=<scheme>`, `identifier=<value>` |

All three are existing native fields per format. Zero new mikebom:* annotations introduced. Constitution Principle V audit passes.

**Alternatives considered**:
- `components[].externalReferences[]` with `type=other`, `url=<scheme>:<value>`, `comment=...` — Rejected: forces non-URI values into a URI-shaped slot; CDX validators may flag non-URI `url` values. The `other` type is also semantic-poor for what is conceptually an identifier, not an external reference.
- New `mikebom:component-identifiers` annotation — Rejected: violates Principle V's native-precedence requirement. Native fields exist for all three formats; use them.
- Mixed: `properties[]` for CDX, `externalRefs[PERSISTENT-ID]` for SPDX — Accepted (this is what the recommended decision encodes). The per-format symmetry isn't perfect (CDX's `properties[]` is structurally different from SPDX's `externalRefs[PERSISTENT-ID]`) but each format uses its idiomatic native field for the semantic.

## §3 — In-toto subject extraction site

**Decision**: Read the subject set at the same point in `mikebom trace run` where the attestation envelope is being assembled — specifically at `cli/run.rs` after `super::scan::execute(scan_args).await?` completes (i.e., after the trace + scan flow has populated the in-process subject collection). The new helper `subject_identifiers_from_attestation_subjects(...)` takes the subject set as input and produces `Vec<Identifier>` for the build-tier identifier vec.

**Rationale**: The trace pipeline already produces a `Vec<Subject>` (or equivalent in-process collection) as part of the witness-v0.1 attestation builder before signing/serializing the envelope. Reading from this collection at SBOM-emit time is a single in-process state transfer; no re-computation, no I/O, no subprocess.

The trace-completion-time read site is the right one because:
- Subjects are deterministically known once the trace is complete (no race with running build).
- The collected subject set is already deduplicated and lexically ordered per witness-v0.1 conventions; we don't need to re-sort.
- The SBOM emission pipeline runs after this point, so the build-tier identifier vec can absorb subjects before any per-format emission begins.

**Implementation note**: The exact field name on the trace-completion in-process state is TBD until implementation; the agent driving T002+ will identify the right field by reading `cli/scan.rs` and `cli/run.rs`. The contract between the helper and its caller is `fn subject_identifiers_from_attestation_subjects(subjects: &[Subject]) -> Vec<Identifier>` regardless of where the input slice is sourced from.

**Alternatives considered**:
- Re-parse the witness-v0.1 attestation JSON after it's serialized — Rejected: round-trips through JSON, slower, lossy if the witness-v0.1 format ever changes.
- Hook into the eBPF event stream directly — Rejected: out of scope; the attestation builder is the canonical post-trace state.
- Have the operator pass `--subject-hash` manually for build-tier too — Rejected: defeats the purpose of auto-detection (one of the milestone's headline values).

## §4 — `subject:` value validation regex

**Decision**: `validate_subject` accepts inputs matching `^(sha256:[0-9a-f]{64}|sha512:[0-9a-f]{128})$` exactly. Anything else fails validation and triggers the FR-005 soft-fail-to-`UserDefined` path.

Specific rejection rules:
- Uppercase hex (`SHA256:ABCD...`) — Reject. RFC 6234 canonical encoding is lowercase; mikebom enforces lowercase for determinism + downstream tool compatibility.
- Mixed-case hex — Reject (same reason).
- Whitespace anywhere — Reject.
- Other algo prefixes (`sha1:`, `blake2b:`, `md5:`, etc.) — Reject; explicitly out of scope per spec assumptions. Future milestone can extend the regex.
- Missing algo prefix (bare hex) — Reject; the value must be self-describing.
- Algo prefix without colon-hex tail — Reject.
- Wrong-length hex (e.g., sha256 algo with 63 hex chars) — Reject.

**Rationale**: A strict, deterministic validator is the right shape for an identifier scheme that downstream tools are expected to string-match against. Loose acceptance (e.g., case-insensitive) creates ambiguity about canonical form and breaks correlation. The two algos covered (sha256, sha512) are the only ones the witness-v0.1 attestation framework canonically emits.

The soft-fail-to-`UserDefined` path means malformed values still ride through the SBOM (under `mikebom:identifiers`-equivalent emission), so operators don't lose data — they just lose the `Builtin` classification + the per-format native carrier.

**Alternatives considered**:
- Case-insensitive hex acceptance (normalize to lowercase before storing) — Rejected: hides operator errors; an operator who typed uppercase may have intended uppercase for a reason. Strict-reject + soft-fail surfaces the issue while preserving the data.
- Accept any algo prefix that matches `[a-z0-9]+` — Rejected: lets typos pass (`shaa256:...` would be accepted as user-defined). Closed enum is safer.
- Algo whitelist via runtime config — Rejected: complexity for no concrete user benefit.

## §5 — Per-component identifier matching algorithm

**Decision**: Byte-equality match against the emitted `components[].purl` field. The operator's `--component-id` selector (the LHS of `=`) must be byte-identical to a component's emitted PURL string for the match to fire.

**Rationale**:
- **Determinism**: byte-equality is unambiguous. PURL-spec-aware semantic equality (e.g., URL-encoding tolerance, version-range matching, type-namespace canonicalization) introduces edge cases and varies by PURL spec version. Byte-equality side-steps these.
- **Operator predictability**: the operator can run a scan first (`mikebom sbom scan --path . --output preview.cdx.json`), `jq -r '.components[].purl'` to see the exact PURLs mikebom emits, and copy those into `--component-id` selectors. Round-trip works trivially.
- **MVP scope**: spec assumptions explicitly call out exact-PURL-match as the MVP; glob/wildcard is future work.

**Edge case handling**:
- PURL with URL-encoded characters: the operator must supply the URL-encoded form that mikebom emits. If mikebom's PURL emission has any non-canonical encoding, the operator's selector must match the non-canonical form. This is fine — byte-equality.
- Multiple components share the same PURL (different `bom-ref` values): all matching components receive the identifier per FR-011.
- Selector matches zero components: warn + continue per FR-010.

**Alternatives considered**:
- PURL-spec semantic equality — Rejected: see Determinism + Operator predictability above. Plus, the `packageurl` crate's normalization rules differ across versions; byte-equality is version-independent.
- `bom-ref`-based selection — Rejected for MVP: bom-refs are derived (not operator-typed) and require running a scan before the operator can supply selectors. PURL is the more user-facing identifier.
- Glob/wildcard PURL matching — Rejected for MVP: scope cut.

## §6 — Determinism contract for multi-component matches

**Decision**: When one `--component-id` selector matches N components, all N receive the identifier in their respective `properties[]` (or per-format equivalent) array. Per-component, the new identifier entries are appended at the end of the existing `properties[]` array, in lexical order by `(name, value)` against any other identifier entries this milestone emits to the same component.

Pre-existing `properties[]` entries on the component (e.g., `mikebom:not-linked` from milestone 049, `mikebom:shade-relocation` from milestone 009) are preserved unchanged; the new entries appear after them.

**Rationale**:
- **Stable serialization**: lexical ordering of new entries gives byte-identical output across re-runs for fixed input.
- **No churn for existing properties**: appending to the existing array preserves byte-identity for components with no `--component-id` flags. Existing 073/074/075 goldens stay byte-identical because no `--component-id` is passed in those fixtures.
- **Multi-component fan-out**: when a single selector matches 5 components, each of the 5 components independently gets the identifier in its own `properties[]` — there's no "shared" entry. The 5 emissions are independent, each lexically sorted within its own component.

**Wire-shape implications**:
- A component with one pre-existing property `mikebom:not-linked=true` plus two new `--component-id` entries `kusari-id:asset-foo` and `acme-asset:bar-prod-001` ends up with:
  ```json
  "properties": [
    {"name": "mikebom:not-linked", "value": "true"},          // pre-existing, position preserved
    {"name": "acme-asset", "value": "bar-prod-001"},          // new, lexically first of new entries
    {"name": "kusari-id", "value": "asset-foo"}               // new, lexically second
  ]
  ```

**Alternatives considered**:
- Sort the entire `properties[]` array (including pre-existing entries) lexically — Rejected: would churn existing goldens that have stable `properties[]` ordering encoded today.
- Insert new entries at array start — Rejected: same churn problem, plus arbitrary placement.
- Per-component identifiers in a separate `mikebom:component-identifiers` array — Rejected: violates Principle V (would be a new `mikebom:*` annotation when the native field works fine).
