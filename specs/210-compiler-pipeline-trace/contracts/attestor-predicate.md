# Contract: `compiler-invocation/v0.1` attestor predicate

> **Superseded in part by milestone 1072 (#1144).** C-1's URI is now
> `compiler-invocation/v0.2`, and C-2's wire shape for `ReadSetEntry` and
> `WriteSetEntry` is replaced by
> `specs/1072-compiler-set-close-time/contracts/read-write-set-shape.md`:
> both entries lost `sha256`, the write set lost `survived_trace_window`
> and gained `renamed_from`, and both gained the close-time tier
> (`existed_at_close`, `sha256_at_close`).
>
> **Every other clause of this contract stands.** This file is a
> historical record of what was decided in July 2026 and its clauses are
> deliberately left unedited — see the superseding contract's header for
> the same statement from the other side.

**Date**: 2026-07-19
**Purpose**: Lock the URI + JSON shape of the new attestor entry so downstream consumers (SBOMit, witness verify, in-toto policy engines, custom parsers) can parse mikebom-produced attestations without breakage across releases.

## C-1: Predicate URI

**Locked**: `https://mikebom.dev/attestation/compiler-invocation/v0.1`

Per Clarifications Q3 — mikebom-owned URI, no upstream coordination overhead. Documented in `docs/architecture/attestations.md`. Version bump (v0.2, v1, etc.) MUST retain the `/compiler-invocation/` path prefix for URI-aliasing forward compatibility.

## C-2: Wire shape (LOCKED)

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [ ... ],
  "predicateType": "https://witness.testifysec.com/attestation-collection/v0.1",
  "predicate": {
    "name": "<step-name>",
    "attestations": [
      {
        "type": "https://mikebom.dev/attestation/compiler-invocation/v0.1",
        "attestation": {
          "invocations": [
            {
              "invocation_id": <u64>,
              "compiler": "rustc" | "gcc" | "clang" | "gpp" | "go" | "ld" | "mold" | "cc1" | "cpp" | "as" | "unknown",
              "pid": <u32>,
              "ppid": <u32>,
              "parent_invocation_id": <u64 | null>,
              "cgroup_id": <u64>,
              "start_timestamp": "<ISO 8601 UTC>",
              "end_timestamp": "<ISO 8601 UTC> | null",
              "argv_full_path": "<abs-path> | null",
              "argv": ["<string>", ...],
              "cwd": "<abs-path> | null",
              "exit_code": <i32 | null>,
              "read_set": [
                {
                  "path": "<abs-path>",
                  "sha256": "<64-hex>",
                  "kind": "file"
                },
                {
                  "path": "<stdin>",
                  "kind": { "stdin_input": { "bytes_read": <u64> } }
                }
              ],
              "write_set": [
                {
                  "path": "<abs-path>",
                  "sha256": "<64-hex> | null",
                  "survived_trace_window": <bool>
                }
              ],
              "events_dropped": <u64>
            }
          ],
          "dag_edges": [
            { "parent_invocation_id": <u64>, "child_invocation_id": <u64> }
          ],
          "completeness": {
            "state": "complete"
          } | {
            "state": "degraded",
            "dropped": <u64>,
            "affected_component_count": <usize>
          } | {
            "state": "partial",
            "reason": "attach_late"
          },
          "secrets_read_filtered": <u64>,
          "include_system_reads_flag": <bool>,
          "filter_categories_applied": ["system" | "user_cache" | "ephemeral" | "secrets_adjacent", ...]
        },
        "subjects": [ ... ]
      }
    ]
  }
}
```

## C-3: Field ordering (LOCKED per FR-011 byte-identity)

- Top-level fields serialize in the order listed above.
- `invocations[]` sorted by `(start_timestamp_ns, pid)` tuple.
- `read_set[]` sorted by `path` lexicographically (byte-order).
- `write_set[]` sorted by `path` lexicographically.
- `dag_edges[]` sorted by `(parent_invocation_id, child_invocation_id)`.
- `filter_categories_applied[]` sorted lexicographically.

## C-4: Backward compatibility

- Fields with `null` values in the wire shape MUST be omitted from JSON via `serde(skip_serializing_if = "Option::is_none")` — pre-m210 attestations without a `compiler_pipeline` section deserialize as `None` cleanly.
- Adding NEW fields to `CompilerInvocation` in future versions is source-compatible: existing consumers ignore unknown fields per `serde_json` default behavior.
- REMOVING a field or changing a field's type is a breaking change — requires URI bump to `v0.2` or higher.

## C-5: Consumer expectations

Downstream consumers of this predicate MUST be able to:
1. Deserialize the `attestation` block into their own data model (they don't have to use mikebom's exact struct names).
2. Reconstruct the DAG from `dag_edges` + `invocations[].parent_invocation_id` (which are redundant on purpose — either alone suffices).
3. Handle `completeness.state == "degraded"` by surfacing the drop-count to their operator (not silently dropping the data).
4. Handle `read_set[i].kind == "stdin_input"` by NOT expecting a `sha256` field (the wire shape omits it).
5. Treat `write_set[i].sha256 == null` as "file was deleted before the trace ended" — NOT as an integrity failure.
