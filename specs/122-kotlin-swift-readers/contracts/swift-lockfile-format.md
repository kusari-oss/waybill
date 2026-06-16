# Contract: `Package.resolved` parsing + Swift PURL projection

**Feature**: 122-kotlin-swift-readers
**Date**: 2026-06-15
**Consumed by**: the Swift reader at `mikebom-cli/src/scan_fs/package_db/swift/lockfile.rs`; integration tests at `mikebom-cli/tests/scan_swift.rs`
**Spec mapping**: FR-001, FR-002, FR-003, FR-009, FR-012, FR-014

## File envelope

The supplement file MUST be a JSON document with a top-level integer `version` field of `1`, `2`, or `3`. Unknown versions produce a `tracing::warn!` line + zero components for that file (FR-009 fail-closed).

```json
{ "version": 2, "pins": [ ... ] }
```

## Per-version schema

### v1 (Swift 5.0 — 5.5; pre-2022 projects)

```json
{
  "object": {
    "pins": [
      {
        "package": "swift-log",
        "repositoryURL": "https://github.com/apple/swift-log.git",
        "state": {
          "branch": null,
          "revision": "32e8d72...",
          "version": "1.4.4"
        }
      }
    ]
  },
  "version": 1
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `version` | integer | ✓ | Must equal `1` |
| `object.pins[]` | array | ✓ | The lockfile's pinned dependencies |
| `object.pins[].package` | string | ✓ | The project's own SwiftPM package name (mikebom uses this as `identity` for the entry) |
| `object.pins[].repositoryURL` | string | ✓ | Drives PURL projection (see § "PURL projection rules" below) |
| `object.pins[].state.revision` | string | ✓ | 40-char hex SHA |
| `object.pins[].state.version` | string \| null | optional | When present + non-null, used as the PURL version segment |
| `object.pins[].state.branch` | string \| null | optional | Ignored by mikebom |

### v2 (Swift 5.6 — 5.10)

```json
{
  "pins": [
    {
      "identity": "swift-log",
      "kind": "remoteSourceControl",
      "location": "https://github.com/apple/swift-log.git",
      "state": {
        "revision": "32e8d72...",
        "version": "1.4.4"
      }
    }
  ],
  "version": 2
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `version` | integer | ✓ | Must equal `2` |
| `pins[]` | array | ✓ | Top-level (no `object` wrapper) |
| `pins[].identity` | string | ✓ | Lowercased package name (mikebom uses as `identity`) |
| `pins[].kind` | string | optional | `"remoteSourceControl"` typical; mikebom doesn't switch on this field |
| `pins[].location` | string | ✓ | Drives PURL projection |
| `pins[].state.revision` | string | ✓ | 40-char hex SHA |
| `pins[].state.version` | string | optional | When present, used as PURL version segment |

### v3 (Swift 5.10+)

Identical shape to v2 plus an OPTIONAL `originHash` field on each pin. mikebom IGNORES `originHash` in v0.1 — it's an integrity signal, not a discovery signal.

```json
{
  "originHash": "...",
  "pins": [ /* same shape as v2 */ ],
  "version": 3
}
```

## Required vs optional pins

mikebom emits a component for every entry in `pins[]` (v2/v3) or `object.pins[]` (v1) that:

1. Has a parseable `location` (v2/v3) or `repositoryURL` (v1).
2. Has a `state.revision` matching `^[0-9a-f]{40}$`.

Entries failing either gate emit a `tracing::warn!` naming the file + the failing field; the walk continues on sibling entries (FR-009).

## PURL projection rules

The PURL projection logic at `swift/lockfile.rs::project_purl(location, version)` produces `pkg:swift/<host>/<namespace>/<name>@<version>` per the purl-spec swift type. Five sub-rules in priority order:

1. **HTTPS-form with `.git` suffix** (the dominant case):
   - Input: `https://github.com/apple/swift-log.git`
   - Output: `pkg:swift/github.com/apple/swift-log@<ver>` (suffix stripped, host preserved)

2. **HTTPS-form without `.git` suffix** (legacy):
   - Input: `https://github.com/apple/swift-log`
   - Output: `pkg:swift/github.com/apple/swift-log@<ver>`

3. **SSH-form** (private internal projects):
   - Input: `git@gitlab.acme.com:internal/lib.git`
   - Output: `pkg:swift/gitlab.acme.com/internal/lib@<ver>`
   - The regex pattern `^(?:(?P<user>[^@]+)@)?(?P<host>[^:]+):(?P<path>.+?)(?:\.git)?$` captures host + path; the user segment is DROPPED per the purl-spec convention.

4. **Deep namespace (GitLab subgroups)**:
   - Input: `https://gitlab.com/group/subgroup/project.git`
   - Output: `pkg:swift/gitlab.com/group%2Fsubgroup/project@<ver>`
   - The middle path segments (everything between `<host>/` and the last `/<name>`) URL-encode the `/` separator so the PURL's three-segment shape is preserved per the purl-spec.

5. **PURL version segment selection** (per clarification Q1 / FR-003):
   - If `state.version` is present + non-empty → use it verbatim
   - Else → use the FULL 40-char `state.revision` SHA, and stamp `mikebom:source-type = "git"` annotation on the component (plus `mikebom:source-revision = "<sha>"` for grep convenience)

## Output: `PackageDbEntry` shape

Each successfully-projected lockfile entry produces ONE `PackageDbEntry` carrying:

| Field | Value |
|---|---|
| `purl` | The projected `pkg:swift/...` PURL per the rules above |
| `name` | `pins[].identity` (v2/v3) or `pins[].package` (v1) |
| `version` | `pins[].state.version` if present, otherwise `pins[].state.revision` |
| `source_type` | `Some("git")` when commit-pinned, `None` otherwise |
| `extra_annotations` | `{ "mikebom:source-files": "<path-to-Package.resolved>" }` plus `{ "mikebom:source-revision": "<sha>" }` when commit-pinned |
| `lifecycle_scope` | `None` (SwiftPM doesn't distinguish runtime/build/test in `Package.resolved`) |
| `sbom_tier` | `Some("source")` (lockfile-locked) |

## Error semantics

| Error class | Cause | Behavior |
|---|---|---|
| `Io { path, source }` | `Package.resolved` unreadable | `tracing::warn!` naming path + the io::Error; reader returns zero components for this file; walk continues |
| `ParseJson { path, source }` | File bytes don't parse as JSON | `tracing::warn!` naming path + the serde_json::Error; zero components; walk continues |
| `UnknownVersion { path, version }` | Top-level `version` field is not 1/2/3 | `tracing::warn!` naming path + the integer; zero components; walk continues |
| `MissingPinsArray { path }` | `pins[]` (or `object.pins[]` on v1) field absent | `tracing::warn!`; zero components; walk continues |
| `InvalidRevision { path, entry_index }` | `state.revision` isn't 40 lowercase hex chars | `tracing::warn!` naming the path + entry index; that specific entry skipped; OTHER entries in the same file continue to emit |
| `UnparseableLocation { path, location, entry_index }` | `location` URL doesn't match any of the projection rules | `tracing::warn!` naming the path + entry index + location; that specific entry skipped; others continue |

Per Constitution Principle III + FR-002 / FR-009, no error condition aborts the scan or yields partial output. Worst case: zero components for the bad file. Best case: every other entry in the same file projects normally.

## Worked example

A `Package.resolved` (v2) declaring three dependencies:

```json
{
  "pins": [
    {
      "identity": "swift-argument-parser",
      "kind": "remoteSourceControl",
      "location": "https://github.com/apple/swift-argument-parser.git",
      "state": { "revision": "abc123def456abc123def456abc123def456abcd", "version": "1.3.0" }
    },
    {
      "identity": "alamofire",
      "kind": "remoteSourceControl",
      "location": "https://github.com/Alamofire/Alamofire.git",
      "state": { "revision": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "version": "5.9.0" }
    },
    {
      "identity": "internal-lib",
      "kind": "remoteSourceControl",
      "location": "git@gitlab.acme.com:internal/lib.git",
      "state": { "revision": "cafebabecafebabecafebabecafebabecafebabe" }
    }
  ],
  "version": 2
}
```

Produces three `PackageDbEntry` records with PURLs:

- `pkg:swift/github.com/apple/swift-argument-parser@1.3.0`
- `pkg:swift/github.com/Alamofire/Alamofire@5.9.0`
- `pkg:swift/gitlab.acme.com/internal/lib@cafebabecafebabecafebabecafebabecafebabe` (commit-pinned full SHA; carries `mikebom:source-type = "git"`)

All three carry `mikebom:source-files = "<path>/Package.resolved"`. The third also carries `mikebom:source-revision = "cafebabe..."` (full SHA, redundant with the version segment, present for grep convenience).
