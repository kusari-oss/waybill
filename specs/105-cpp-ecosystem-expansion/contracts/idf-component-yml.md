# Contract: `idf_component.yml` reader (US4)

**Maps to**: FR-006 | **Source-mechanism**: `idf-component` / `idf-component-local` | **New module**: `mikebom-cli/src/scan_fs/package_db/idf_component.rs`

## Trigger

Any file named `idf_component.yml` anywhere under the scan root. Typical esp-idf
layout produces multiple instances: `main/idf_component.yml`,
`components/<name>/idf_component.yml`.

## Parsing

`serde_yaml::from_str::<IdfComponentManifest>`. The schema:

```yaml
dependencies:
  espressif/mdns: "^1.2.0"            # registry, version range
  espressif/esp_websocket_client: "1.4.2"  # registry, exact version
  my_lib:
    path: ../my_lib                   # local path
  remote_lib:
    git: https://github.com/foo/bar
    version: v1.0.0                   # git source
```

## PURL derivation (per clarification Q2)

| Dependency form | PURL |
|---|---|
| Registry `<ns>/<name>: "<version>"` | `pkg:idf/<ns>/<name>@<version>` |
| Registry with version range (`^1.2.0`) and **no** `dependencies.lock` present | `pkg:idf/<ns>/<name>@<range-string>` + `mikebom:requirement-range: "^1.2.0"` |
| Registry with version range AND `dependencies.lock` present | exact pinned version from lockfile |
| Local path | `pkg:generic/<name>` |
| Git source | `pkg:git+https://<sanitized-url>@<version>` |

## Fallback identity (per clarification Q2)

Every registry-form component MUST carry a `mikebom:download-url` annotation
naming the upstream source URL (typically the GitHub repo for the component,
extracted from the manifest's `repository:` field if present, or from a
sibling `idf_component.yml` `url:` field, or — as a last resort — recorded
as `https://components.espressif.com/<namespace>/<name>` placeholder). This
gives consumers that don't yet recognize `pkg:idf/` a source-URL fallback.

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"idf-component"` (registry) or `"idf-component-local"` (path) |
| `mikebom:source-files` | absolute path of the `idf_component.yml` |
| `mikebom:download-url` | per "Fallback identity" above |
| `mikebom:requirement-range` | (optional) original version range string when no lockfile resolved |

## Multi-manifest union (per US4 scenario 3)

The reader runs once per `idf_component.yml`. The dedup pipeline (FR-015)
collapses identical PURLs across multiple manifests — typical esp-idf trees
have e.g. `espressif/mdns` declared in both `main/idf_component.yml` and a
nested component manifest; mikebom emits a single component with the
`mikebom:source-files` annotation listing all files that declared it
(reusing the existing comma-joined source-files convention).

## Test cases (US4 acceptance scenarios mapped)

| US4 Scenario | Fixture | Assertion |
|---|---|---|
| 1 (exact version) | `golden_inputs/idf_component/exact/` | `pkg:idf/espressif/mdns@1.4.2` |
| 2 (range + lockfile) | `golden_inputs/idf_component/locked/` | exact version from `dependencies.lock` |
| 3 (multi-manifest union) | `golden_inputs/idf_component/multi/` | one component per PURL across all manifests |
| 4 (path-based local) | `golden_inputs/idf_component/local/` | `pkg:generic/my_lib` + `idf-component-local` |

## Boundaries

- The Espressif Component Registry is NOT contacted at scan time (FR-012).
- Range-to-version resolution is local-file only (`dependencies.lock`).
- `service_url:` override is recorded as a `mikebom:registry-url` annotation but does not change the PURL host.
