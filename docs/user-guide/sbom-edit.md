# Editing an SBOM

Generate once, derive what you distribute. `waybill sbom scan` describes
everything it found. What you hand to a customer, an auditor or a public
registry is usually less:

- no development and test dependencies;
- no internal file paths;
- no internal hostnames;
- no names of private packages.

`waybill sbom edit` derives that version from the full one. The derivative
says what it is: it carries the original's hash and signature and the
operations applied, so a recipient can check it against the original
without seeing anything the edit removed.

```bash
# 1. Generate and sign, as usual.
waybill sbom scan --path ./product --output cyclonedx-json=full.cdx.json --sign-key ./vendor.pem

# 2. Derive the distributable version.
waybill sbom edit full.cdx.json -o customer.cdx.json \
  --drop 'scope=development,test' \
  --drop 'tier=file' \
  --drop-annotations 'waybill:' \
  --redact paths \
  --redact 'hosts:pseudonymise=*.corp.acme.example' \
  --redact 'names:pseudonymise=@acme/*' --redact-key-file ./redact.key \
  --sign-key ./vendor.pem

# 3. Whoever holds the original can verify the chain.
waybill sbom verify-chain customer.cdx.json --original full.cdx.json --key ./vendor.pub
```

The editor works on CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 JSON, and writes
the format it reads. The same operations on the three formats of one scan
give three documents that agree under `waybill sbom parity-check`. Flag
details are in the [CLI reference](cli-reference.md#waybill-sbom-edit).

## Operations

Operations apply in command-line order. Each reports how many items it
matched and changed. One that matches nothing is reported, not an error.

### Dropping components: `--drop <selector>`

A selector is `;`-separated `key=value[,value...]` terms. Terms combine with
AND; values within a term combine with OR.

| Key | Matches |
|---|---|
| `purl` | the PURL, as a glob |
| `ecosystem` | the PURL type: `npm`, `cargo`, `deb`, … |
| `scope` | `runtime`, `development`, `build`, `test`, `optional`, `excluded` |
| `tier` | the component's tier: `source`, `design`, `analyzed`, …, or `file` |
| `role` | the component's role (`waybill:component-role`), its type, or its closure role (`waybill:closure-role`: `artifact-input`, `build-tooling`, `both`, `unreferenced`) |
| `name` | the name, as a glob, or `re:<regex>` |

Examples: `scope=development,test`, `ecosystem=npm;name=@acme/*`, `tier=file`,
`role=build-tooling`.

`--exclude-scope` belongs to `sbom scan` and is refused here. Use
`--drop 'scope=…'` instead.

`scope=excluded` matches components that are not in the artifact at all, as
opposed to dependencies with a non-runtime lifecycle scope. Each format has a
native mark for this:

| Format | A component matches `scope=excluded` when |
|---|---|
| CycloneDX | it has `"scope": "excluded"` and no `waybill:lifecycle-scope` or `waybill:optional-derivation` property |
| SPDX 2.3 | it is the `spdxElementId` of a `BUILD_TOOL_OF` relationship |
| SPDX 3 | it is a `to` of a `LifecycleScopedRelationship` with `relationshipType: usesTool` and `scope: build` |

The marked components are the members of a `--nix-closure` scan that only
build the artifact, and, in CycloneDX only, Go modules that `go mod why`
proved unneeded (`waybill:build-inclusion: not-needed`). CycloneDX also writes
development, build, test and optional dependencies as `"scope": "excluded"`.
For those, the lifecycle property says which scope they have, so they match
that scope and not `excluded`. The one case this cannot express is a component
that has a lifecycle scope and is also not in the artifact. CycloneDX has a
single field for both, so `scope=excluded` misses that component in CycloneDX
and matches it in SPDX.

Dropping a component removes everything that refers to it:

- its dependency edges;
- annotations and relationships about it;
- licence and vulnerability entries that only it used;
- vulnerability statements about it. A CycloneDX build statement names the
  root in `affects[]` and the patched component in a
  `waybill:vex-subcomponent` property; once its last subcomponent is dropped,
  the statement goes too, since the root alone would be a different claim.
  The count is reported on stderr.

Its dependents are connected to its dependencies, so nothing reachable before
becomes unreachable. A bridged edge keeps a non-runtime scope: a dependency
reached through a dev-only link stays dev-only.

A changed dependency list is no longer known complete, and the document says
so:

- CycloneDX: an `incomplete` composition;
- SPDX 3: `completeness: incomplete` on the relationship;
- all three formats: `waybill:graph-completeness` becomes `unknown`.

A selector that matches the document's root is refused: the result would
describe nothing.

### Runtime-only view of a Nix closure

A `--nix-closure` document lists the whole build closure: the evaluated
package, what ships with it, and every compiler, build tool and script used
to build it. Members that don't ship carry the not-in-artifact mark, so one
command derives the runtime-only document:

```bash
waybill sbom edit closure.cdx.json   --drop 'scope=excluded' -o runtime.cdx.json
waybill sbom edit closure.spdx.json  --drop 'scope=excluded' -o runtime.spdx.json
waybill sbom edit closure.spdx3.json --drop 'scope=excluded' -o runtime.spdx3.json
```

The three commands drop the same PURLs.

**Why the drop is needed.** grype, Trivy and osv-scanner don't read
CycloneDX `scope`, so a mark alone changes no scanner result: every build
tool in the full document is scanned as if it shipped. On the five wrapper
flakes measured (curl, openssh, httpie, gh, ripgrep), grype reports 1,301
findings on the full documents and 350 on the runtime-only ones. Those 350
are exactly the findings on components in each package's runtime closure:
none lost, none added (`specs/1423-nix-runtime-scope/measurements/us4-scanner-check/`).

The edit removes VEX statements about the dropped members and keeps the
rest; stderr reports how many went. For SPDX, the scan writes VEX to an
OpenVEX sidecar, which `sbom edit` does not change (see [Limits](#limits)).

**How exact the marks are.** The document-scope `waybill:closure-scope-method`
annotation (catalogue row C216) says how the marks were decided:

```bash
jq -r '.metadata.properties[] | select(.name == "waybill:closure-scope-method") | .value' closure.cdx.json
# {"method":"runtime-closure","schema":1,"static-c-linking-detected":false,
#  "static-link-source":"none","store":"local","unavailable-reason":null,
#  "unmatched-runtime-paths":0,"verified":true,"walk-outcome":null}
```

| `method` | Where the marks come from | How far to trust them |
|---|---|---|
| `runtime-closure` | The evaluated output's runtime closure, `nix path-info -r`, from the local store or a substituter your nix already trusts (`store`). A substituter's answer is signature-checked first (`verified`). | Exact for what the output references. Go modules, Rust crates and Haskell libraries compiled into a runtime member are kept, with `waybill:scope-classification-source` `statically-linked`. |
| `reachability-walk` | No store held the output (`unavailable-reason` says why), so waybill walked from the evaluated package over runtime-linking dependency fields. | An approximation. On the five flakes above it kept 346 findings, 331 of them runtime, and missed 19 runtime ones (`measurements/us2-walk-score/`). |
| `closure-role` | Neither source was usable; each member's build-graph role decides. | The weakest. Before milestone 1423 every closure document was marked this way, and as a findings filter it was 31.1% precise on the same five flakes (`measurements/README.md`). |

With no `waybill:closure-scope-method` at all, the closure tier did not run
(for example under `--offline`), nothing is marked, and the recipe drops
nothing.

Two things none of the methods detect:

- **Static C linking** (`pkgsStatic`, vendored `.a` archives). A C library
  linked statically leaves no store reference, so it is marked like any other
  build input. `static-c-linking-detected: false` says so.
- **Components that are not closure members.** The flake's own inputs, such
  as `pkg:github/nixos/nixpkgs`, are source references with lifecycle scope
  `build`. They carry no not-in-artifact mark, so the recipe keeps them.

**The CycloneDX limit.** CycloneDX has one field for both "not in the
artifact" and "a non-runtime lifecycle scope" (see the table above). A closure
member that is not in the artifact and also has a lifecycle scope, for
example one a lifecycle pass scoped as development, is written as
`scope: excluded` with a `waybill:lifecycle-scope` property, which reads as
the lifecycle scope. `scope=excluded` matches it in SPDX but not in CycloneDX,
so the CycloneDX runtime-only document keeps it. On the five flakes above
this did not occur: all three formats dropped the same PURLs. Run the recipe
on an SPDX document as well when you need to be sure.

### Dropping annotations: `--drop-annotations <namespace>`

This removes annotations whose field starts with the namespace (`waybill:`,
`waybill:graph-`, …). Two are never removed:

- `waybill:generation-context`: every document states which mode produced it (constitution, Operating Modes);
- `waybill:derivation`: the record this command adds.

### Redacting: `--redact <class>[:<mode>][=<pattern>]`

| Class | What is replaced | Default mode | Pattern |
|---|---|---|---|
| `paths` | every path-bearing field: source files, file paths, evidence locations, workspace members, read sets, … | `remove` | optional; all paths if omitted |
| `hosts` | hostnames matching the pattern, wherever they appear (URLs, identifiers, creators) | `pseudonymise` | required |
| `names` | component names matching the pattern, in every form the document writes them: plain, in PURLs (`%40acme/…`), in CPEs (`\@acme\/…`), and in the component's own download URLs | `pseudonymise` | required |

The two modes:

- **`remove`** replaces each value with an opaque marker:
  - `redacted-path-<n>` for paths;
  - `redacted-host-<n>.invalid` for hosts;
  - `redacted-<n>` for names.

  Distinct values keep distinct markers, so the components stay distinct.
- **`pseudonymise`** replaces each value with `redacted-` plus an HMAC of the
  value under your key (`--redact-key-file`). The same value under the same
  key gives the same token in every document. A recipient can correlate
  components across your SBOMs without learning their names.

Rewritten PURLs still parse as PURLs, and hosts stay valid hostnames under
the reserved `.invalid` top-level domain.

Redaction fails closed. After replacing, waybill searches the whole output,
including inside base64-encoded material, for every form it replaced. If any
remains, nothing is written.

#### Keys

The key file is read as bytes, with a trailing newline ignored. It is never
written to the output or logged. Keep it secret: anyone holding it can test a
guessed name against a pseudonym. Rotate it to break linkability with SBOMs
you have already published.

#### What redaction does not hide

Redaction is not anonymisation. A redacted component can still be identified
from what remains:

- **Content hashes** of a public package identify it exactly.
- **Version strings, licences and download sizes** narrow it down.
- **The dependency graph's shape** is itself a fingerprint. A component with
  one well-known dependency set is often recognisable.
- **Pseudonyms are linkable** by design, across documents and across time,
  by anyone; and they are reversible by guessing, by anyone holding the key.
- **The derivation record's original hash** lets anyone holding a candidate
  original confirm it was the source.

Drop what must not be identified, rather than only redacting it.

## The derivation record

Every edited document carries a `waybill:derivation` annotation (catalogue
row C194):

```json
{
  "schema": "waybill-derivation/v1",
  "original": {
    "sha256": "…",
    "format": "cyclonedx-1.6",
    "signature": { "kind": "jsf", "embedded": true, "material": { … } }
  },
  "operations": [
    { "category": "drop-components", "matched": 2, "changed": 2 },
    { "category": "redact-hosts", "matched": 1, "changed": 1 }
  ],
  "ancestors": [],
  "tool": "waybill 0.11.0",
  "created": "2026-10-06T00:00:00Z"
}
```

The record holds counts, never values.

- **`original.signature`** holds the original's signature material without
  any signed payload. If that material contains a value the edit redacted,
  such as a keyless certificate naming an internal host, it holds only the
  material's digest, with `reason: contains-redacted-values`.
- **`ancestors`** holds the original's own record, when the original was
  itself edited.

Each format also links to the original natively:

| Format | Link | The derived document's identity |
|---|---|---|
| CycloneDX | root `externalReferences[]` entry of type `bom`, `url: urn:sha256:<hash>`, with a SHA-256 hash | same `serialNumber`, `version` incremented |
| SPDX 2.3 | `externalDocumentRefs[]` entry `DocumentRef-original` with its SHA-256, and `SPDXRef-DOCUMENT AMENDS DocumentRef-original:SPDXRef-DOCUMENT` | new `documentNamespace` |
| SPDX 3 | an `import` `ExternalMap` for the original document, verified by SHA-256, and a `Relationship` original `amendedBy` derived | new `SpdxDocument` IRI; elements keep theirs |

The original's own signature does not survive an edit, because it signs
bytes that no longer exist. Its material moves into the record. Sign the
output with `--sign-key` or `--sign`, exactly as with `sbom scan`.

## Verifying a chain

```bash
waybill sbom verify-chain customer.cdx.json \
  --original full.cdx.json --key vendor.pub
```

For each step, newest first, this reports three checks:

- **own signature:** the document's signature, in-document (CycloneDX JSF) or
  in its sidecar (`.sig.json` DSSE, `.sig.bundle.json` Sigstore);
- **original hash:** whether the supplied original is the one the record names;
- **original signature:** the signature the record carries, checked against
  the supplied original's bytes.

Pass `--original` once per step to follow a chain of edits. A step without
its original is reported as `original-not-supplied`, not failed.

Static-key signatures verify only against keys you pass with `--key`. A key
embedded in a signature is never trusted, since anyone can re-sign with
their own. Passing any `--key` also requires the derived document itself to
be signed, so a stripped signature fails rather than reading as unsigned.

Keyless (Sigstore) signatures report **`delegated`**. waybill checks that
the bundle signs this document's digest, but it holds no Sigstore trust
root. So the certificate chain, the signer's identity and the transparency
log entry are for `cosign`, and verify-chain prints the exact
`cosign verify-blob` command to run. A delegated check is never counted as
verified.

The exit status is non-zero if any check failed or mismatched.

## Limits

- **One document per invocation.** The OpenVEX sidecar a scan writes next
  to an SPDX document (`<name>.openvex.json`) is not edited. It names
  components by PURL, so edit or withhold it separately when you drop or
  redact components.
- **SPDX 2.3 key order.** waybill writes SPDX 2.3 in field order, not sorted.
  An edited SPDX 2.3 document is written with sorted keys: the same JSON,
  reordered. CycloneDX and SPDX 3 keep their bytes outside what changed.
- **Not yet.** Policy files describing a standard edit, and re-identification
  of pseudonyms for key holders, are planned for a follow-up milestone.
