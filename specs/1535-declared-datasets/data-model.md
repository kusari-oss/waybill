# Data model: declared datasets

Everything is in process for one scan, in the CMake reader.

## DeclaredDownload (per `file(DOWNLOAD …)` in a discovered CMake file)

| Field | Meaning |
|---|---|
| `file` | the declaring CMake file |
| `url_template` | the first argument, raw |
| `resolution` | `Literal(url)`, `ListLoop { list, elements }`, or `Unresolved` (FR-001/FR-004) |
| `digest` | when a same-file literal list is taken at the same index in the same loop and checked by `file(<ALG> …)`, the per-element `(alg, hex)` |

**How the list loop is recognised.** Two loop shapes qualify:
- `foreach(<v> IN LISTS <L>)`, where `<L>` is a same-file literal `set(<L> …)`;
- `foreach(<i> RANGE …)` with `list(GET <L> ${<i>} <v>)`, where `<L>` is a same-file literal list.

The URL's only variable must be `<v>`. Elements are substituted in order.

## Dataset (emitted, per distinct element)

| Field | Value |
|---|---|
| `purl` | `pkg:generic/[<ns>/]<name>?download_url=<first-url>&checksum=<alg>:<hex>` (research R3) |
| `name` | the element after its last `/` |
| `version` | absent |
| `sbom_tier` | `source` (digest declared) |
| `evidence` | every declaring file, as `source_path` and `extra_source_paths` |
| `component-kind` | C160 `dataset` |
| type | CycloneDX `data`; SPDX 3 `dataset_DatasetPackage` with `software_primaryPurpose: "data"` and `dataset_datasetType: ["noAssertion"]`; SPDX 2.3 `primaryPackagePurpose: OTHER` |

**Identity.** Two elements are the same dataset when their element text matches exactly. Fallback URLs and repeat declarations join the same component; the first URL and the first digest win.

**Digest disagreement.** When two declarations of the same element give different digests, neither is recorded, and the case is counted in `digest-mismatches`.

## Classification (FR-008, research R2)

A download in a `ListLoop` with a paired digest list is a **dataset**. Any other resolved download is **not classified**: it is counted and emits nothing.

## C229 `waybill:declared-downloads` (document scope)

```json
{"datasets":N,"digest-mismatches":N,"not-classified":N,"read":N,"unresolved":N}
```

| Key | Counts |
|---|---|
| `read` | `file(DOWNLOAD)` calls read |
| `unresolved` | FR-004 |
| `not-classified` | resolved calls that are not datasets |
| `datasets` | distinct dataset components |
| `digest-mismatches` | per element |

Present only when `read` > 0.
