# Contract: `waybill sbom edit` and `waybill sbom verify-chain`

## `waybill sbom edit <INPUT> -o <OUTPUT> [OPERATIONS] [SIGNING]`

Reads one CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 JSON document, applies the operations in order, and writes the same format.

### Operations (repeatable, applied in order)

| flag | meaning |
|---|---|
| `--drop <selector>` | Drop the matching components and every reference to them. Bridge their dependents to their dependencies, and downgrade completeness claims. |
| `--drop-annotations <namespace>` | Remove annotations whose field starts with `<namespace>` (e.g. `waybill:` or `waybill:graph-`). The protected set, `waybill:generation-context` (C21) and `waybill:derivation`, is never removed. |
| `--redact <class>[:<mode>][=<pattern>]` | `class` is `paths`, `hosts` or `names`. `mode` is `remove` or `pseudonymise` (default `remove` for paths, `pseudonymise` for hosts and names). `pattern` is a glob, or a regex prefixed `re:`; for `paths`, omitting it means every path. |
| `--redact-key-file <path>` | The pseudonymisation key. Required when any operation pseudonymises; never written anywhere. |

### Selector syntax

A selector is a `;`-separated list of `key=value[,value...]` terms. Terms combine with AND; values within a term combine with OR. Keys:

- `purl`: glob;
- `ecosystem`: PURL type;
- `scope`: `runtime`, `development`, `build` or `test`;
- `tier`;
- `role`;
- `name`: glob, or `re:<regex>`.

Examples: `scope=development,test`, `ecosystem=npm;name=@acme/*`, `tier=file`.

### Signing (same as `waybill sbom scan`)

`--sign-key <pem>` [`--sign-key-passphrase-env <VAR>`], or keyless `--sign` with its existing companions.

`--original-signature <path>` names the original's sidecar when it isn't next to the input under the standard name.

### Exit status and output

- **0:** the output was written. A per-operation report (`matched`, `changed`) goes to stderr.
- **Non-zero, nothing written:**
  - an invalid operation or selector;
  - an unsupported input format or version;
  - a selector that would drop the root or subject;
  - pseudonymisation without a key;
  - a failed post-condition: a dropped identifier or a redacted value still present, or a reference that doesn't resolve.
- **An operation that matched nothing** is reported, and is not an error.

## `waybill sbom verify-chain <DERIVED> [--original <file>]... [--key <pem>]...`

Checks the derived document's own signature, and each derivation link. `--original` is given once per step, newest first. `--key` supplies the public keys for static-key signatures. Prints a `ChainReport` (see data-model.md); `--json` gives machine-readable output.

**Exit status:** 0 only if no check `Failed` or `Mismatched`. A `Delegated` check (a keyless certificate or Rekor check, done with the printed `cosign` command) and an `OriginalNotSupplied` check are reported, not failed, and never counted as verified.
