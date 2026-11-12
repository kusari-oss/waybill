# Contract: closure-backport VEX and the `nix_drv` qualifier

## The qualifier

```
pkg:generic/unzip@6.0?nix_drv=<32-char drv store hash>[,<another>…]   # sorted; every patching variant
```

- **On:** only components with a CVE-resolving backport. The value lists every patching derivation merged into the component (research R1, R4).
- **Claim:** a `fixed` statement for a CVE only when every listed derivation resolves it.
- **Across formats:** the same string in CycloneDX `purl` and `bom-ref`, SPDX 2.3 `externalRefs[PACKAGE-MANAGER/purl]`, SPDX 3 `software_packageUrl`, and the OpenVEX product `@id`.
- **Determinism:** the same derivation always gives the same value.
- **Matching:** scanners strip or ignore qualifiers when matching advisories (measured: grype 0.120.0, `q_none` = `none` on five goldens; osv-scanner 2.7.0, 0 of 67 findings lost with every PURL qualified).

## OpenVEX (sidecar `waybill.openvex.json`)

Before (m1035), per CVE:

```json
{"vulnerability":{"name":"CVE-2014-8139"},"products":[{"@id":"pkg:generic/unzip@6.0"}],"status":"affected", ...}
{"vulnerability":{"name":"CVE-2014-8139"},"products":[{"@id":"<root>","subcomponents":[{"@id":"pkg:generic/unzip@6.0"}]}],"status":"not_affected", ...}
```

After, per (component, CVE):

```json
{"vulnerability":{"name":"CVE-2014-8139"},
 "products":[{"@id":"pkg:generic/unzip@6.0?nix_drv=<hash>","identifiers":{"purl":"pkg:generic/unzip@6.0?nix_drv=<hash>"}}],
 "status":"fixed",
 "impact_statement":"<m1035 grade note: resolved by a nixpkgs backport applied to this build; association <grade> ...>"}
```

- There is no root dependency: the statement is emitted whether or not the document names a subject.
- m1050 declaration statements are unchanged (FR-008).

## CycloneDX `vulnerabilities[]`

```json
{"id":"CVE-2014-8139",
 "affects":[{"ref":"pkg:generic/unzip@6.0?nix_drv=<hash>"}],
 "analysis":{"state":"resolved","detail":"<same note>"}}
```

- There is exactly one entry per (component, CVE).
- No `exploitable` entry exists for a backported pair.
- No `waybill:vex-subcomponent` property is emitted for backports.

## C220 `waybill:vex-claims-unscoped`

- **Value:** a document-scope decimal string counting the backport claims left without a statement because only some of the component's derivations resolve the CVE.
- **When:** emitted only when greater than zero.
- **Carriers:** CycloneDX `metadata.properties[]`; SPDX 2.3 document annotation; SPDX 3 document `Annotation`. `SymmetricEqual`.
- **Catalog:** KEEP-NO-NATIVE, the same audit as C193: no format has a slot for "claims this document could not scope".
