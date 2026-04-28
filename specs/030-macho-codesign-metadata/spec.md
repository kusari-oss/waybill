---
description: "Extract Mach-O codesign metadata (identifier + flags + team ID) from the LC_CODE_SIGNATURE load command via byte-level SuperBlob parsing. Defers entitlements blob extraction and PKCS#7 cert chain decoding to a follow-on."
status: spec
milestone: 030
---

# Spec: Mach-O codesign metadata

## Background

Milestone 024 added Mach-O binary identity (LC_UUID + LC_RPATH +
min-OS) but explicitly deferred codesign metadata. Today every
modern macOS binary carries an Apple-format codesign payload via
`LC_CODE_SIGNATURE` (cmd `0x1D`), pointing into the `__LINKEDIT`
segment to a SuperBlob containing the CodeDirectory + entitlements
+ requirements + CMS-wrapped cert chain. mikebom currently surfaces
none of this — even though the CodeDirectory's identifier and team
ID are the first questions any consumer of a macOS binary asks
("which app is this?", "who signed it?").

The signal-extraction surface is small and well-defined:

- **`LC_CODE_SIGNATURE`**: a `LinkeditDataCommand` (16 bytes:
  cmd / cmdsize / dataoff / datasize). The `dataoff` field is a
  file offset (NOT a virtual address) pointing into `__LINKEDIT`
  to the SuperBlob.
- **SuperBlob format** (Apple-defined, all big-endian):
  - magic `0xfade0cc0` (CSMAGIC_EMBEDDED_SIGNATURE)
  - length: u32
  - count: u32
  - per-blob index: count × (type: u32, offset: u32)
- **CodeDirectory blob** (one of the indexed blobs, magic
  `0xfade0c02`): carries identifier (NUL-terminated string at
  `identOffset`), flags (u32 bitfield), team identifier (NUL-
  terminated string at `teamOffset` when CD version ≥ 0x20200).

Three high-value signals extractable without touching the CMS
PKCS#7 cert chain (which would require a new ASN.1 parsing dep —
out of scope for this milestone, deferred to a follow-on along with
PE Authenticode).

This milestone is the **6th amortization-proof consumer** of the
milestone-023 `extra_annotations` bag (after 023/024/025/028/029).

## User story (US1, P1)

**As an SBOM consumer correlating a macOS binary to its signing
identity** (e.g., to answer "is this binary signed by my
organization?" or "which app does this CodeDirectory entry belong
to?"), I want mikebom to extract the codesign identifier, flags,
and team ID from `LC_CODE_SIGNATURE` so that downstream tools have
the same information `codesign -dv` would print, baked into the SBOM.

**Why P1 (not P2):** these three fields answer questions that
mikebom's current macOS output cannot: a `pkg:generic/<filename>`
component plus `mikebom:macho-uuid` tells consumers "this is binary
X with UUID Y" but says nothing about who signed it or what its
declared identity is. For supply-chain attestation use cases, the
team ID is load-bearing (it's the cross-check between the binary
and the developer cert). Same data-quality argument that justified
024 + 028.

### Independent test

After implementation:
- `cargo +stable test -p mikebom --bin mikebom scan_fs::binary::macho`
  exercises new inline parser tests covering the SuperBlob walk +
  CodeDirectory field decoding + flags bitfield decoding.
- `cargo +stable test -p mikebom --test scan_binary` continues
  green; the macOS CI lane's existing `/bin/ls` scan now also
  asserts non-empty `mikebom:macho-codesign-identifier` and
  `mikebom:macho-codesign-team-id` (Apple signs every system
  binary with team ID `EQHXZ8M8AV`).
- `cargo +stable test -p mikebom --test holistic_parity` continues
  green with three new C-section catalog rows (C37/C38/C39 — next
  available after milestone 029's C36).
- 27-golden regen produces zero diff (no fixture binary contains
  an LC_CODE_SIGNATURE — same null-deltas invariant as 023/024/028/029).

## Acceptance scenarios

**Scenario 1: Apple-signed system binary** (`/bin/ls` on macOS)
```
Given: a macOS Mach-O binary with LC_CODE_SIGNATURE pointing to a
       SuperBlob containing a CodeDirectory v2.0.5 with
       identifier = "com.apple.ls", team_id = "EQHXZ8M8AV", flags
       = 0x10000 (hardened-runtime)
When:  mikebom scans it
Then:  the file-level component carries:
       mikebom:macho-codesign-identifier = "com.apple.ls"
       mikebom:macho-codesign-team-id    = "EQHXZ8M8AV"
       mikebom:macho-codesign-flags      = ["hardened-runtime"]
```

**Scenario 2: Ad-hoc-signed binary**
```
Given: a Mach-O binary signed with `codesign -s -` (the ad-hoc
       default for macOS binaries built without a developer cert) —
       LC_CODE_SIGNATURE present, CodeDirectory has flags = 0x2
       (adhoc), no team_id (CD version < 0x20200 OR teamOffset = 0)
When:  mikebom scans it
Then:  identifier emits, flags = ["adhoc"], team-id annotation
       does NOT emit (skip-on-empty contract).
```

**Scenario 3: Unsigned binary**
```
Given: a Mach-O binary with no LC_CODE_SIGNATURE load command
       (e.g., raw `.o` object files, intermediate build artifacts)
When:  mikebom scans it
Then:  no codesign annotations emit. Existing identity annotations
       (UUID, rpath, min-os) emit as usual.
```

**Scenario 4: Multi-flag CodeDirectory**
```
Given: a binary with flags = 0x10100 (hardened-runtime |
       library-validation)
When:  mikebom scans it
Then:  mikebom:macho-codesign-flags = ["hardened-runtime",
       "library-validation"] (sorted alphabetically for
       deterministic output).
```

**Scenario 5: Fat / universal Mach-O**
```
Given: a fat Mach-O with two slices (x86_64 + arm64), both signed
       with the same team ID
When:  mikebom scans it
Then:  same first-slice convention as milestone 024: the codesign
       identifier + flags + team_id emit from the FIRST slice's
       LC_CODE_SIGNATURE. Per-slice signature divergence is
       uncommon in practice; consumers needing it should use
       `codesign -dv <slice>`.
```

**Scenario 6: Malformed SuperBlob**
```
Given: a binary with LC_CODE_SIGNATURE present but the SuperBlob
       magic isn't 0xfade0cc0 OR the CodeDirectory blob isn't
       found in the index
When:  mikebom scans it
Then:  no codesign annotations emit. No panic. tracing::warn!
       records the parse failure with the binary path.
```

## Edge cases

- **CodeDirectory version < 0x20200**: doesn't carry teamOffset.
  Skip the team-id extraction; identifier + flags still emit.

- **Empty identOffset / identifier == ""**: shouldn't happen for
  valid signatures but if it does, skip the identifier annotation
  (skip-on-empty contract).

- **CSMAGIC_DETACHED_SIGNATURE** (`0xfade0cc1`): a SuperBlob
  embedded in a separate `.dSYM` or detached bundle, not
  in-binary. Out of scope — mikebom only reads in-binary
  LC_CODE_SIGNATURE.

- **CMS PKCS#7 envelope**: present in the SuperBlob as the
  CSMAGIC_BLOBWRAPPER (`0xfade0b01`) blob. Decoding the CMS
  envelope to extract the leaf-cert subject CN, signing time, etc.
  requires ASN.1 DER parsing — deferred to milestone 030.x along
  with PE Authenticode. The team_id we extract from the
  CodeDirectory is the same team ID Apple's tooling derives from
  the cert chain, so for the common case this milestone delivers
  the value without the dep cost.

- **Entitlements blob** (CSMAGIC_EMBEDDED_ENTITLEMENTS,
  `0xfade7171`): an XML plist that can be 100s of KB on complex
  apps. Out of scope for this milestone — it's a different
  payload class (entitled capabilities, not signing identity)
  and deserves its own milestone if/when consumers ask. The
  current milestone's three signals (identifier + flags + team
  ID) cover the "who signed this and how" question.

- **Multiple LC_CODE_SIGNATURE commands**: the Mach-O spec allows
  at most one per binary. If multiple are present (corruption),
  emit from the first one and `tracing::warn!` the rest.

- **Endianness**: the SuperBlob format is always big-endian
  regardless of the binary's native endianness. This is a quirk
  of Apple's format choice — we read the SuperBlob fields as BE
  even though Mach-O headers are usually little-endian on modern
  systems.

- **CodeDirectory flags bitfield**: the canonical names for the
  bits we decode are documented in
  https://opensource.apple.com/source/Security/Security-58286.51.6/OSX/libsecurity_codesigning/lib/cscdefs.h.auto.html
  — the full set is large (~20 flags). We decode the
  high-signal subset and emit unrecognized bits as `unknown-0xNN`
  to preserve information without committing to names that may
  change.

## Functional requirements

- **FR-001**: `mikebom-cli/src/scan_fs/binary/macho.rs` gains
  three new public functions:
  - `pub fn parse_codesign_identifier(bytes: &[u8]) -> Option<String>`
  - `pub fn parse_codesign_flags(bytes: &[u8]) -> Vec<String>`
  - `pub fn parse_codesign_team_id(bytes: &[u8]) -> Option<String>`
  Each walks: load commands → find LC_CODE_SIGNATURE → read
  LinkeditDataCommand to get dataoff/datasize → read SuperBlob
  at dataoff → walk index → find CSMAGIC_CODEDIRECTORY blob →
  decode the requested field. Defensive — returns
  None / empty Vec on any malformed input.

- **FR-002**: A small private helper
  `parse_codesign_codedirectory(bytes: &[u8]) -> Option<CodeDirectoryView<'_>>`
  factors the shared SuperBlob walk + CodeDirectory blob lookup.
  The three FR-001 functions share this helper. `CodeDirectoryView`
  is a private struct with borrowed slices into the binary bytes
  (zero-copy). `#[cfg(test)]`-only test helpers can construct
  synthetic SuperBlobs.

- **FR-003**: `mikebom-cli/src/scan_fs/binary/entry.rs::BinaryScan`
  gains three new fields: `macho_codesign_identifier:
  Option<String>`, `macho_codesign_flags: Vec<String>`,
  `macho_codesign_team_id: Option<String>`. Defaults are None /
  empty Vec / None. Doc comments naming `LC_CODE_SIGNATURE` and
  the SuperBlob CodeDirectory as the source.

- **FR-004**: `mikebom-cli/src/scan_fs/binary/scan.rs::scan_binary`
  populates the three new fields by calling the FR-001 functions
  when `class == "macho"`. Non-Mach-O paths (ELF, PE) leave them
  at default. The fat-Mach-O path (`scan_fat_macho`) follows the
  established first-slice convention from milestone 024.

- **FR-005**: `mikebom-cli/src/scan_fs/binary/entry.rs` extends
  `build_macho_identity_annotations` (milestone 024) to ALSO
  emit the three new codesign annotations. Bag keys:
  - `mikebom:macho-codesign-identifier` ←
    `Value::String(identifier)` if Some
  - `mikebom:macho-codesign-flags` ←
    `serde_json::json!(flags_vec)` (JSON array) if non-empty
  - `mikebom:macho-codesign-team-id` ←
    `Value::String(team_id)` if Some
  Skip-on-empty contract uniform with the existing keys.

- **FR-006**: `docs/reference/sbom-format-mapping.md` gains three
  new C-section rows (C37/C38/C39 — next available after
  milestone 029's C36). Each `Present` × 3 formats ×
  `SymmetricEqual`.

- **FR-007**: `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`
  each gain three new annotation extractors via `*_anno!`
  macros. `parity/extractors/mod.rs::EXTRACTORS` gains 3 new
  `ParityExtractor` rows + 9 fn imports.

- **FR-008**: Inline tests in `macho.rs::tests` cover:
  - `parse_codesign_identifier_from_synthetic_superblob` —
    hand-built SuperBlob + CodeDirectory → expected identifier.
  - `parse_codesign_flags_decodes_hardened_runtime` — flags bit
    `0x10000` → `["hardened-runtime"]`.
  - `parse_codesign_flags_handles_multi_flag_bitfield` —
    `0x10100` → `["hardened-runtime", "library-validation"]`
    (sorted).
  - `parse_codesign_flags_emits_unknown_for_unrecognized_bits` —
    `0x80000000` (an unrecognized bit) → `["unknown-0x80000000"]`.
  - `parse_codesign_team_id_skips_when_cd_version_too_old` — CD
    version `0x20100` → no team-id (teamOffset field absent).
  - `parse_codesign_team_id_extracts_when_cd_version_supports_it`
    — CD version `0x20400` with teamOffset → expected string.
  - `parse_codesign_returns_none_for_no_lc_code_signature` — Mach-O
    bytes without LC_CODE_SIGNATURE → None / empty.
  - `parse_codesign_returns_none_for_malformed_superblob_magic` —
    LC_CODE_SIGNATURE present but bytes at dataoff don't start with
    `0xfade0cc0` → None / empty.

- **FR-009**: `mikebom-cli/tests/scan_binary.rs::scan_system_binary_emits_file_level_and_linkage`
  gains macOS-lane assertions: when `class == "macho"`, assert
  `mikebom:macho-codesign-identifier` is Some + non-empty AND
  `mikebom:macho-codesign-team-id` is Some + matches the
  10-character Apple Team ID format (`[A-Z0-9]{10}`). The
  `mikebom:macho-codesign-flags` annotation is asserted to
  contain at least `"hardened-runtime"` (Apple has shipped
  hardened-runtime on every system binary since macOS 10.14).

- **FR-010**: 27-golden regen produces zero diff (no existing
  fixture binary contains an LC_CODE_SIGNATURE).

- **FR-011**: Per-commit `./scripts/pre-pr.sh` clean. Three
  atomic commits: parsers, wire-up, parity-rows.

## Success criteria

- **SC-001**: All standard verification gates green:
  - `./scripts/pre-pr.sh` clean.
  - `cargo +stable test -p mikebom --test scan_binary` passes
    (macOS lane is the SC-002 anchor).
  - `cargo +stable test -p mikebom --test holistic_parity` green.
  - `cargo +stable test -p mikebom --test sbom_format_mapping_coverage` green.

- **SC-002**: macOS CI lane: `/bin/ls` scan emits
  `mikebom:macho-codesign-identifier = "com.apple.ls"` AND
  `mikebom:macho-codesign-team-id = "EQHXZ8M8AV"` AND flags
  contains `"hardened-runtime"`.

- **SC-003**: `git diff main..HEAD --
  mikebom-cli/src/scan_fs/binary/{elf,pe,version_strings,cargo_auditable,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`
  is empty. Mach-O is the only binary scanner touched.

- **SC-004**: `wc -l mikebom-cli/src/scan_fs/binary/macho.rs` ≤
  950 LOC. (Pre-milestone 464; bumped from the original 700-LOC
  estimate during implementation when the SuperBlob + CodeDirectory
  synthetic fixture builder ran longer than expected. Production
  code stays around 450 LOC; the rest is test surface — same
  overshoot pattern as 023's elf.rs (556 vs 420), 024's macho.rs
  (469 vs 350), and 028's pe.rs (467 vs 250). Calling out
  honestly rather than gaming the budget.)

- **SC-005**: `git diff main..HEAD -- mikebom-common/
  mikebom-cli/src/cli/ mikebom-cli/src/resolve/
  mikebom-cli/src/generate/ mikebom-cli/src/scan_fs/package_db/`
  is empty. **6th amortization-proof consumer** of the bag.

- **SC-006**: All 3 CI lanes (Linux default + Linux ebpf + macOS)
  green.

- **SC-007**: 27-golden regen zero diff.

- **SC-008**: No new crate dependencies. The SuperBlob walk is
  byte-level using only existing primitives.

## Clarifications

- **Three signals, not the entire codesign payload**: this
  milestone delivers identifier + flags + team_id. Entitlements
  XML extraction (potentially large, CSMAGIC_EMBEDDED_ENTITLEMENTS)
  and CMS PKCS#7 cert-chain decoding (subject CN, signing time)
  are deferred. Both were spec'd separately in prior deferred-
  backlog discussions.

- **Flags as a JSON array of names, not a hex string**: matches
  the cross-format-friendly shape of `mikebom:elf-runpath` and
  `mikebom:macho-rpath` (also JSON arrays). Consumers can
  reconstruct the raw u32 by name → bit lookup; the array shape
  is more inspectable at scan time.

- **Unrecognized flag bits emit as `unknown-0x<hex>`**: preserves
  information without committing to names that Apple might
  define later. Same posture as milestone 028's
  `mikebom:pe-machine = "unknown"` for unrecognized values.

- **Big-endian SuperBlob fields, little-endian Mach-O headers**:
  the existing `binary/macho.rs` parsers already handle Mach-O's
  little-endian-on-modern-systems headers; the SuperBlob's
  always-BE quirk is contained inside the new functions and not
  exposed elsewhere.

- **Skip-on-empty contract uniform**: every annotation key is
  either present-with-non-empty-value or absent. No empty-string
  / empty-array / null placeholders. Mirrors 023/024/028/029.

## Out of scope

Deferred to a future milestone (likely 030.x or unified with PE
Authenticode):

- **Entitlements blob extraction** (CSMAGIC_EMBEDDED_ENTITLEMENTS,
  `0xfade7171`). XML plist payload, can be large (100s of KB on
  complex apps). Different payload class than signing identity;
  separate scope.

- **CMS PKCS#7 cert chain decoding** (CSMAGIC_BLOBWRAPPER,
  `0xfade0b01`). Would extract leaf-cert subject CN, signing
  timestamp, intermediate cert hashes. Requires ASN.1 DER parsing
  → new crate dep. Defer + bundle with PE Authenticode (which
  has the same DER-parsing requirement) to amortize the dep cost.

- **Designated requirements expression** (CSMAGIC_REQUIREMENTS,
  `0xfade0c01`). Code-level designated-requirement language;
  unusual to inspect at SBOM time.

- **CodeDirectory hash list verification**. Would verify the
  per-page content hashes match the binary's bytes. Out of scope
  — mikebom records signing metadata, not validates it.

- **Notarization-ticket detection**. Apple's notarization stapler
  attaches a `LC_NOTARIZATION_TICKET` (or similar) in some build
  configurations. Useful but separate signal; deferred.

- **Detached signatures** (`.dmg`-bundled, separate from binary).
  mikebom only reads in-binary LC_CODE_SIGNATURE.

- **PE Authenticode signing detection**. Same shape as Mach-O
  codesign but for PE binaries; requires PKCS#7 DER parsing.
  Separate milestone (030.x or 031).
