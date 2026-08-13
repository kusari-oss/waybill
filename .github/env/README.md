# `.github/env/`

Single source of truth for pinned tool versions consumed across
multiple CI workflows + local scripts.

Each `*.env` file follows shell-style `KEY=VALUE` conventions:

- Parseable by `source` in bash.
- Parseable by `docker build --build-arg` via a small extractor
  (`grep KEY= file | cut -d= -f2`).
- Consumable by GitHub Actions steps that read the file, extract the
  value, and either `echo "$KEY=$VALUE" >> "$GITHUB_ENV"` or pass the
  value as an input to another action.

## Current files

- **`bpf-linker.env`** — pinned `BPF_LINKER_VERSION` for the eBPF
  build path. See `docs/development/ebpf-toolchain.md` for the full
  bump / un-pin flow. Ownership: m234 milestone.

## Adding a new pin

1. Create `.github/env/<tool>.env` with `TOOL_VERSION=<version>` +
   a header comment enumerating the consumers.
2. Add a consumer in a workflow via either (a) a composite action
   under `.github/actions/install-<tool>/` OR (b) a step that reads
   the env file directly.
3. Add a `pin-consistency.yml` guard entry so ad-hoc
   `--version <literal>` inlining anywhere else in the repo trips CI.
4. Document in `docs/development/`.
