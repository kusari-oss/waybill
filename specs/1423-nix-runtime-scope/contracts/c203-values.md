# Contract: C203 `waybill:scope-classification-source` (extended)

The existing per-component row. Its closed enum gains three values. Carriers, parity extractor (`SymmetricEqual`) and classification are unchanged.

| value | meaning | native mark |
|---|---|---|
| `runtime-closure` | decided by the runtime closure (in it → runtime; not in it → not-in-artifact) | `scope: excluded` / `BUILD_TOOL_OF` / `usesTool` only when not-in-artifact |
| `statically-linked` | not in the runtime closure, but linked into a runtime member (Go/Rust embedded, or a Haskell library); or, when a walk is set aside as `reached-only-root`, the Haskell build a wrapper root consumes and the libraries linked into it | none (runtime) |
| `reachability-walk` | decided by the fallback walk | marked when the walk did not reach it |

## Rules

- Every closure member carries exactly one C203 value after the pass. That now includes runtime members, which carried none before.
- `statically-linked` never appears together with a not-in-artifact mark.
- `runtime-closure` and `reachability-walk` appear on both marked and unmarked members. The native mark, not C203, says which.

## I10 changes

1. **Population.** It was "role is `build-tooling` or `unreferenced`". It becomes every component with `waybill:closure-role`.
2. **Passing.** A component passes if C203 is an exempt value, or if C203 is one of the three new values and its native mark agrees:
   - `statically-linked` → unmarked;
   - otherwise, marked iff the native mark is present (checked against itself, not against the role).
3. **Converse check.** No component with C203 `runtime-closure` or `statically-linked` carries `scope: excluded` unless the first case applies.
4. **Pre-1423 documents** still pass under today's rule when C203 is `closure-role`.
