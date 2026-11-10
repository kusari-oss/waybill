# Contract: CLI

## `--nix-closure` (existing, behaviour extended)

Now also reads the outputs of closure members that are already present in the
store for embedded Go and Rust build information. No new flag enables this.
Nothing is built or fetched.

## `--nix-closure-require-root-contents` (new)

```
waybill sbom scan --nix-closure --nix-closure-require-root-contents --path .
```

- Boolean, default off. Requires `--nix-closure` (clap `requires`).
- When set, the scan fails with a non-zero exit and **writes no document** if
  the closure root's contents could not be read:
  - the root's read state is `absent`, `failed` or `no-outputs`;
  - the closure has no single root;
  - the closure tier degraded.
- A root that was read and embeds nothing is not a failure.
- A `partial` root passes with a `WARN`.
- Unread build inputs never fail the scan.

Error message names the root and the reason, for example:

```
Error: --nix-closure-require-root-contents: the closure root `app 1.2.3` could not be read (absent): its output is not present in /nix/store. Build or fetch it first.
```

## Without the flag

An unread root logs one `WARN` naming the root and its read state. The scan
succeeds.

## Store location

`NIX_STORE_DIR`, when set, is where outputs are looked for; otherwise
`/nix/store`.

## Log line

One `INFO` per scan: `nix-closure: contents` with `members`, `read`,
`partial`, `absent`, `failed`, `yielded`, `modules`, `root`.
