# Wire contract: declared datasets

## A dataset component

| | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| Kind | `type: "data"` | `primaryPackagePurpose: OTHER` | element `type: dataset_DatasetPackage`, `software_primaryPurpose: "data"`, `dataset_datasetType: ["noAssertion"]` |
| Identity | `purl` `pkg:generic/<ns>/<name>?download_url=…&checksum=md5:…` | `externalRefs` PURL | `software_packageUrl` |
| Hash | `hashes[]` from the `checksum` (MD5 here) | `checksums[]` | `verifiedUsing[]` |
| Version | absent | `versionInfo: NOASSERTION` | absent |
| C160 `waybill:component-kind` | `dataset` | `dataset` | `dataset` |
| Evidence | the declaring CMake files (C18) | the same | the same |

**The hash is of the declared download** (the archive), not of any installed file.

**Validation:** SPDX 3 output must pass `spdx3-validate`, and CycloneDX its 1.6 schema (SC-002).

## C160, extended

The `waybill:component-kind` closed enum gains `dataset`: a third-party dataset whose download a build declares. The row text is extended; the row and its extractors are unchanged.

## C229 `waybill:declared-downloads` (document scope, new)

A JSON object encoded as a string with sorted keys (data-model). It is present when the CMake reader read at least one `file(DOWNLOAD)`.

| Format | Carrier |
|---|---|
| CycloneDX | `metadata.properties` |
| SPDX 2.3 | document annotation |
| SPDX 3 | document annotation |

**Principle V:** no format counts the download declarations a scanner read and why each did or did not become a component.
