# Contract: what a trace attestation records about files

Applies to both attestation formats (`waybill-v1` and `witness-v0.1`) wherever they carry file operations, and to `waybill-v1`'s `compiler_pipeline`.

## File operations (`file_access.operations[]`)

| case | `path` | `operation` | `unresolved_relative` |
|---|---|---|---|
| absolute open, read-only | as opened | `read` | absent |
| absolute open with `O_WRONLY`, `O_RDWR`, `O_CREAT` or `O_TRUNC` | as opened | `write` | absent |
| relative open; the opener's directory is known | directory joined with the path, lexically | `read` or `write` by flags | absent |
| relative open; the directory is unknown, or the path is relative to a directory fd | as the build passed it | `read` or `write` by flags | `true` |
| successful rename | the new path, resolved as for opens | `write` | `true` if the new path is unresolved |

The kernel no longer drops an open because its path is relative. It also no longer drops paths under a build directory's `deps/`. Paths under `.fingerprint/` and `incremental/` are still dropped, as are the other milestone 213 categories.

## Compiler pipeline (`compiler_pipeline.invocations[]`)

- `read_set`: every resolved path the invocation opened for reading. **No unresolved path, ever.**
- `write_set`: every resolved path the invocation opened for writing.
  - A rename by the invocation of a path already in its write set replaces that entry with the new path.
  - A rename of a file the invocation did not write leaves the write set unchanged.
  - No unresolved path, ever.

## Trace integrity (`trace_integrity`)

- `unresolved_relative_opens`: the number of operations recorded with `unresolved_relative: true`. Omitted when zero.

## Products in `witness-v0.1`

Products are built from `write` operations (`attestation/witness_builder.rs`). They were always empty because no open was ever classified as a write. With this change they list what the build wrote. That is the intended effect, not a side effect.

## Unchanged

- Every attestation field not named above.
- C130/C131 emission: no production path emits them yet (#1142).
- Paths are not canonicalised. `..` segments and symlinked directories are reported as the build named them.

## Example (fixture, illustrative shape)

```json
{
  "path": "/src/two_binaries_diverge/libsafe/src/lib.rs",
  "operation": "read",
  "process": {"pid": 24023, "tid": 24023, "comm": "rustc"},
  "size": 0,
  "timestamp": "2026-10-06T04:34:06.696Z"
}
```

An open that cannot be resolved:

```json
{
  "path": "raw-dylibs",
  "operation": "read",
  "unresolved_relative": true,
  "process": {"pid": 24030, "tid": 24030, "comm": "rustc"},
  "size": 0,
  "timestamp": "2026-10-06T04:34:06.701Z"
}
```
