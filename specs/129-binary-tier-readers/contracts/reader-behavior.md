# Reader behavior contract — milestone 129

Per-reader input / output / side-effect / observability contract. Tests at
`mikebom-cli/tests/binary_tier_us{1,1,2,3}*.rs` exercise these contracts end-to-end via
`mikebom sbom scan`.

## Reader 1: `.deps.json` reader (US1)

**Module path**: `mikebom-cli/src/scan_fs/package_db/dotnet/deps_json.rs`

### Input

- Scan rootfs (a directory tree, typically `<tempdir>/rootfs/` after image extraction).
- The reader walks via `safe_walk` (milestone 114) for paths matching `*.deps.json` extension.
- Each matching file is opened, byte-buffered, and parsed via `serde_json::from_slice::<DotnetDepsJsonDocument>`.

### Output

- `Vec<PackageDbEntry>`, one entry per `libraries` map entry whose `LibraryType::Package` variant fires.

### Side effects

- None. The reader is pure I/O-in / data-out. The parsed entities have no mutable state.

### Logs

- `debug`-level: one line per `.deps.json` file successfully parsed, naming the file path and the count
  of emitted entries.
- `warn`-level: one line per malformed `.deps.json` (parse failure, malformed key, unknown `LibraryType`).
  Naming the file path and the parse error. The scan does NOT abort.
- `warn`-level: one line per declared-but-not-installed library (FR edge case). Annotation
  `mikebom:image-presence = "declared-not-installed"` set on the component.

### Invariants

- `--offline` honored: zero network calls.
- `--exclude-path` honored: the `safe_walk` helper does the path-filtering centrally.
- Reader does not abort the surrounding scan on any parse failure.

---

## Reader 2: PE/CLR managed-assembly reader (US1)

**Module path**: `mikebom-cli/src/scan_fs/binary/dotnet_pe.rs`

### Input

- Scan rootfs.
- The reader walks via `safe_walk` for paths matching `*.dll` extension.
- Each matching file is opened via `object::read::pe::PeFile::parse`.
- `is_managed_assembly()` check (`DataDirectory[14]` non-zero) GATEs all subsequent metadata parsing.

### Output

- `Vec<PackageDbEntry>`, one entry per `.dll` that:
  - Has a valid CLR header (`DataDirectory[14]` non-zero).
  - Has a parseable `Assembly` metadata-table row.
  - Is NOT covered by a higher-fidelity `.deps.json` declaration in the same scan (per FR-011 dedup).

### Side effects

- None.

### Logs

- `debug`-level: per managed assembly successfully parsed; logs `name`, `version`, `path`.
- `debug`-level (no entry per skip): when a `.dll` is detected to be a native (non-CLR) DLL, the reader
  silently returns early. Per Principle X transparency, the existence of a `.dll` mikebom can't read
  is not surfaced because doing so on every native DLL in `/usr/lib/` would generate noise without
  signal.
- `warn`-level: per parse failure inside a CLR-tagged `.dll` (corrupt metadata table, missing
  `#Strings` heap, etc.). Names the file path. Scan does NOT abort.

### Invariants

- Same as Reader 1 (offline, exclude-path-aware, non-aborting).
- `is_managed_assembly()` MUST return `bool` cheaply (~1 µs per `.dll`) — gated read of a single
  data-directory entry.
- Per FR-011: if a `.deps.json` declaration covers the same `(name, version)`, the PE-derived
  component is SUPPRESSED — the milestone-105 dedup pipeline handles this. Reader emits both
  unconditionally; dedup resolves the collision downstream.

---

## Reader 3: cargo-auditable binary reader (US2)

**Module path**: `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`

### Input

- Scan rootfs.
- The reader walks via `safe_walk` for ELF files (gated by the existing milestone-096 `is_elf` byte-magic
  helper from `symbol_fingerprint.rs`).
- Each ELF file is opened via `object::read::elf::ElfFile::parse`.
- `find_section(file, ".dep-v0")` returns `Option<&Section>`. If `None`, reader silently returns —
  no log.
- If present, section bytes are deflate-decompressed via `flate2::read::DeflateDecoder` and parsed via
  `serde_json::from_slice::<CargoAuditablePayload>`.

### Output

- `Vec<PackageDbEntry>`, one entry per `packages[]` entry. Per FR-016: `kind: "runtime"` (default) →
  emit without scope tag. `kind: "build"` → `lifecycle_scope = Build`. `kind: "dev"` →
  `lifecycle_scope = Test` (per clarification Q1).

### Side effects

- None.

### Logs

- `debug`-level: per ELF successfully parsed AND with `.dep-v0` section AND with valid payload; logs
  `path`, package count.
- `debug`-level (silent skip): when `.dep-v0` section is absent, no log entry per FR-020. (Otherwise
  we'd log every binary in `/usr/bin/` once.)
- `warn`-level: per ELF whose `.dep-v0` section fails to decompress or parse. Names file path and the
  parse error. Scan does NOT abort.

### Invariants

- Same as Reader 1.
- `is_elf()` MUST return `bool` from the first 4 magic bytes (`0x7f 'E' 'L' 'F'`) without reading the
  rest of the file.
- 32-bit + 64-bit ELF + x86_64 + aarch64 + arm + riscv64 all supported via the `object` crate's
  cross-arch handling (FR-019).
- The `flate2` decoder reads the raw deflate stream — no gzip frame, no zlib header. The cargo-auditable
  v0 spec mandates raw deflate.

---

## Reader 4 (extension): Maven nested-JAR recursion (US3)

**Module path**: `mikebom-cli/src/scan_fs/package_db/maven/jar.rs` — extends the existing milestone-009
top-level reader with a recursive `walk_nested_archives` helper.

### Input

- The existing top-level reader already enumerates top-level `.jar` files in the rootfs and parses their
  `META-INF/maven/.../pom.properties` entries.
- The new path: for each top-level `.jar` file's `zip::ZipArchive` iteration, when an entry's name ends
  in `.jar` / `.war` / `.ear` (clarification Q2), extract the entry's bytes into a `Vec<u8>` and pass
  them to `walk_nested_archives(&inner_bytes, depth=1, &mut visited, &mut out, &outer_path)`.

### Output

- Additional `Vec<PackageDbEntry>` entries (appended to the existing top-level emissions) — one per
  nested `pom.properties` entry, with `mikebom:source-mechanism = "maven-jar-nested"`.

### Side effects

- None.

### Logs

- `debug`-level: per nested archive successfully descended; logs outer path, inner path, depth.
- `warn`-level: per archive exceeding the 1 GB decompressed-size cap (FR-025). Names outer + inner path.
- `warn`-level: per nested-archive depth limit (8) being reached (FR-021). Names outer path.
- `warn`-level: per archive cycle detected (FR-024). Names outer path + the recurring SHA-256 hash.

### Invariants

- Same as Reader 1.
- 8-level depth limit (FR-021, matches milestone-128 `INCLUDE_DEPTH_LIMIT`).
- SHA-256-keyed visited set (FR-024).
- 1 GB per-archive size cap (FR-025).
- Only `.jar` / `.war` / `.ear` extensions descended (FR-022, clarification Q2). `.zip` NOT descended.
- Existing top-level reader's behavior unchanged — only the recursive path is added.

---

## Cross-reader integration via the scan-orchestrator

All four reader paths plug into the existing `scan_fs::scan_path` orchestrator at
`mikebom-cli/src/scan_fs/mod.rs`. The orchestrator:

1. Walks the rootfs once via the existing top-level walker.
2. For each detected file, dispatches to the matching reader based on extension + content-magic byte.
3. Collects the emitted `Vec<PackageDbEntry>` from all readers.
4. Routes the combined list through the milestone-105 dedup pipeline.
5. Constructs `Vec<ResolvedComponent>` and passes it to the format emitters.

No changes to the orchestrator are needed for milestone 129 — the new readers just register themselves
in the existing dispatch table. Specifically:

- Add `dotnet::deps_json::read_all` to the `package_db::read_all` dispatch (same shape as the existing
  `cargo::read_all`, `maven::read_all`, etc.).
- Add `binary::dotnet_pe::read_all` and `binary::cargo_auditable::read_all` to the `binary::read_all`
  dispatch.
- Maven nested-JAR is an EXTENSION of the existing `maven::jar::read_all`; no new dispatch entry.
