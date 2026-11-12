# Feature Specification: GHC boot-library versions from the compiler the closure names

**Feature Branch**: `966-ghc-boot-versions`
**Created**: 2026-11-12
**Status**: Draft
**Input**: User description: "966 GHC boot-library versions on the closure tier. Refs #966. Measured 2026-11-12 (findings on #966): boot-library versions vary per GHC patch release (base and process change within every multi-release series), so a per-series table is wrong. The closure tier already records the exact compiler (ghc@9.10.3 on moat/shellcheck/slack-web/slacklinker, ghc@9.6.6 on openai), yet emits its boot libraries as versionless compiler-supplied pkg:hackage components (24-29 per closure golden). Add a committed, dated table keyed by ghc-X.Y.Z, produced by a committed probe: GHC release notes' "Included libraries" first, Stack global-hints.yaml for names the notes omit, disagreements settled from the ghc-X.Y.Z-release tag (one known Stack error: 9.12.3 process is 1.6.26.1, not 1.6.26.0). Verify the table against every available GHC package.conf.d (store and cache listings matched 37/37 and 42/42). On the closure tier, a boot library takes its version from the table at the GHC that built the member; a GHC version absent from the table keeps compiler-supplied (Principle IX). Use the exact GHC's table entries as the boot set, so os-string is no longer compiler-supplied on a 9.6.6 closure. The static (m926) tier is unchanged. Success: on the five closure goldens, every boot library the GHC ships gets a version (24/24, 25/26, 26/26, 26/27, 27/29; the remainder are not GHC-shipped), and osv-scanner reports HSEC-2023-0007 on base. Zero new Cargo deps, no scan-time network."

Every figure below comes from `measurements/README.md` (the findings posted on #966) unless it says otherwise.

## Background

GHC ships a set of Haskell libraries inside the compiler: `base`, `process`, `bytestring`, `Cabal` and about 40 more. nixpkgs marks them by binding them to `null` in `configuration-ghc-<series>.nix` ("use the one that comes with GHC"), so the package set has no version to give. Milestone 926 reports them as `compiler-supplied` (C170), and milestone 1275 emits the closure tier's copies the same way (FR-012c). Both are versionless and explained. Principle IX forbids inventing a version.

#966 asked whether the version could be *established* instead. The research answered yes, under one condition: the exact GHC patch version must be known.

- **A series isn't enough.** `base` and `process` change between patch releases in every series that has more than one release. In 9.12 alone, 21 libraries differ across its 4 releases.
- **The closure tier knows the exact compiler.** Every Haskell closure golden records it: `ghc@9.10.3` on moat, shellcheck, slack-web and slacklinker, and `ghc@9.6.6` on openai, each alongside a separate bootstrap `ghc-binary`.
- **The version is a published fact for each compiler release.**
  - GHC's release notes and Stack's `global-hints.yaml` agree on 943 of 944 entries across 26 releases.
  - The one disagreement (Stack's 9.12.3 `process`) is settled by GHC's own release tag, which agrees with the release notes.
  - The package database of every GHC available locally matches (37 of 37 and 42 of 42 libraries), and so does cache.nixos.org's listing of the same store path.

What is unversioned today in those goldens is **not only the closure tier's own components**:

| target | versionless `pkg:hackage` | emitted by the design tier (m926, from the project's `.cabal`) | emitted by the closure tier (FR-012c) |
|---|---|---|---|
| nix-closure-moat | 24 | 19 | 5 |
| nix-closure-openai | 26 | 22 | 4 |
| nix-closure-shellcheck | 26 | 0 | 26 |
| nix-closure-slack-web | 27 | 24 | 3 |
| nix-closure-slacklinker | 29 | 25 | 4 |

`base` itself is a design-tier component in all four mixed targets, and on moat it carries C171 `9.10.x,9.4.x,9.6.x`: the static tier could not tell which compiler was used. The closure can. So this feature applies to every boot-library component in a scan where **the closure tier established the compiler**, whichever reader emitted it. A scan without a closure is unchanged.

Research also found that the boot *set* is wrong where the compiler is known. On openai (GHC 9.6.6), `os-string` is reported `compiler-supplied`, but 9.6.6 does not ship it. It is nulled only in the 9.10+ configurations, and m926's union over candidate series took those configurations in.

## Clarifications

### Session 2026-11-12

- Q: Which boot-library components get versioned when the closure identifies the compiler? → A: Every boot-library component in the document, whether the design tier (m926) or the closure tier emitted it. Scans without `--nix-closure` are unchanged (FR-005, FR-011). "The static tier is unchanged" means the static tier on its own.
- Q: A closure holds the project's GHC and a bootstrap `ghc-binary`, and both supply `base`. Which version does a boot library take? → A: One component per compiler. A boot library takes the version of the GHC that built the member depending on it. When two compilers ship a library at different versions, both versioned components appear, each with edges only from members its compiler built. A library both compilers ship at the same version stays one component (FR-009).
- Q: Does a design-tier boot library that takes its version from the closure's compiler keep its tier label? → A: Yes, it stays `design`. The tier says where the dependency was declared, and FR-007's record says where the version came from. This follows m926's precedent: design-tier components already take versions from nixpkgs and stay design tier.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A vulnerability scanner can match the libraries GHC put in my program (Priority: P1)

A user scans their Nix-built Haskell project with `--nix-closure` and feeds the document to grype or osv-scanner. `base`, `process` and the other libraries the compiler linked in carry the versions that compiler actually shipped. The scanner can therefore match advisories against them.

**Why this priority**: it is the point of the feature. A versionless component matches no advisory. Today `base`, which every Haskell program links, is invisible to every scanner, and HSEC-2023-0007 affects every `base` release.

**Independent Test**: scan moat with `--nix-closure` and check that `base` is `pkg:hackage/base@4.20.2.0`. Then run osv-scanner on the document and check that it reports HSEC-2023-0007 against it.

**Acceptance Scenarios**:

1. **Given** a closure whose compiler is GHC 9.10.3, **When** it is scanned, **Then** every boot-library component that GHC ships carries 9.10.3's version of it, whether the design tier or the closure tier emitted it.
2. **Given** that document, **When** osv-scanner reads it, **Then** it reports HSEC-2023-0007 on `base`.
3. **Given** a boot-library component that gained a version, **When** a reader inspects it, **Then** the component records where the version came from (the compiler, and the table that gave it), and no longer carries the "no version" reasons C170 and C151.

---

### User Story 2 - The compiler-supplied set matches the compiler I used (Priority: P1)

The same user's project is built with GHC 9.6.6. A library that 9.6.6 does not ship, such as `os-string`, must not be reported as compiler-supplied just because a later GHC series ships it.

**Why this priority**: the compiler-supplied set drives three decisions besides the version: the version-resolution exemption (m926), the scope exemption (m1276), and which absent declared targets get a component (FR-012c). A wrong member is wrong in all three.

**Independent Test**: scan openai with `--nix-closure` and check that `os-string` is not `compiler-supplied`. It is then resolved as any other declared dependency is, from the pinned package set.

**Acceptance Scenarios**:

1. **Given** a closure whose compiler is GHC 9.6.6, **When** it is scanned, **Then** the compiler-supplied set is exactly the libraries 9.6.6 ships on that platform, and `os-string` is not in it.
2. **Given** a scan with no closure, **When** it is scanned, **Then** the compiler-supplied set is m926's union rule, unchanged.

---

### User Story 3 - A compiler waybill has no data for is reported honestly (Priority: P2)

The user's closure uses a GHC released after the table was last refreshed.

**Why this priority**: the table goes stale with every GHC release. When that happens, the scan must say why the libraries have no versions, rather than guess or stay silent.

**Independent Test**: scan a closure whose compiler is not in the table. Check that its boot libraries stay versionless and `compiler-supplied`, and that the document says the compiler was known but had no table entry.

**Acceptance Scenarios**:

1. **Given** a closure compiler absent from the table, **When** it is scanned, **Then** no boot library gains a version, the compiler-supplied set falls back to m926's rule, and the document records which compiler had no entry.
2. **Given** the table, **When** a reader asks where it came from, **Then** the table states its sources, the date it was observed, and how to regenerate it.

### Edge Cases

- **Two compilers in one closure.** Every closure golden holds the project's GHC and a bootstrap `ghc-binary` (9.8.4, or 9.2.4 on openai), which builds bootstrap tools such as `happy-lib`. Those tools are confirmed members with declared edges to `base`.
  - Each compiler's version is emitted as its own component, and each edge points at the version its member's compiler shipped (FR-009).
  - No version is chosen between two (Principle IX), and none is discarded (the #1216 class).
  - The bootstrap compiler's copies are reached only through build tools, so the existing closure scope pass decides their scope. This feature doesn't.
- **Platform-specific libraries.** The tables list `Win32`, but a Linux or darwin GHC doesn't ship it; the store package databases measured on darwin lack it. `Win32` appears as a component today (haskell-language-server's golden, static tier). A library the compiler doesn't ship on the scanned platform MUST NOT gain a version from the table.
- **A library the table omits for that compiler.** For example, `rts` and `ghc-bignum` appear in Stack's table but not in the release notes. The plan decides how those are filled (see FR-002). If no source gives a version, the library stays `compiler-supplied`.
- **A compiler patched or overridden by an overlay.** A flake can replace the compiler, but its derivation still names a version, and that is the version looked up. A boot library whose version an overlay changed without changing the compiler's version would be misreported. This is accepted as out of scope, because no measured target does it. The plan's verification against real package databases (FR-004) is what would reveal it.
- **The static tier with exactly one candidate series.** It could map the series to a patch release at the pinned revision (`ghc96 = ghc967`), but an overlay can replace the compiler and the static tier can't see overlays. It stays unchanged (out of scope).
- **The design tier and the closure tier emit the same library.** Today both emit the versionless `pkg:hackage/base`, and they merge into one component. Versioning one must not leave a versionless duplicate beside a versioned one, which is the #1216 defect class.
- **An advisory for another platform.** HSEC-2024-0003 (`process` before 1.6.23.0) affects only Windows, and openai's GHC 9.6.6 ships `process` 1.6.19.0. With a true version, a scanner will report it. That is a correct match against the advisory's ranges; whether it applies is a VEX question, outside this feature.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST carry a committed table mapping each GHC release `X.Y.Z` to the libraries it ships and their versions. The table MUST be keyed by the exact release, never by series, and MUST record its sources and the date they were observed.
- **FR-002**: The table MUST be produced by a committed, re-runnable probe, ordered as follows:
  - GHC's release notes ("Included libraries") first;
  - Stack's `global-hints.yaml` for libraries the notes omit;
  - any disagreement between them settled from GHC's own `ghc-X.Y.Z-release` tag and recorded.

  It MUST cover at least every GHC release that a pinned revision in the corpus can produce. The known disagreement (9.12.3 `process` is 1.6.26.1) MUST come out right.
- **FR-003**: The table MUST state, per library, whether it is platform-specific, so that a Windows-only library (`Win32`) is never versioned on a non-Windows scan.
- **FR-004**: A test MUST check the table against the package database of every GHC available to it. Where the database is present, every library it holds MUST agree with the table, and the table MUST NOT list for that platform a library the database lacks. Where none is present, the test MUST say so rather than pass silently.
- **FR-005**: When the closure tier ran and identified the compiler that built the scanned output, every boot-library component in the document MUST carry a version from the table, whether the design tier or the closure tier emitted it. The version comes from the compiler FR-009 assigns: the scanned output's compiler for the project's own declarations, and the building compiler for a closure member's. Its PURL MUST carry the version.
- **FR-006**: In the same case, the compiler-supplied set used by the version-resolution exemption (m926), the scope exemption (m1276) and FR-012c MUST be the libraries the table lists for that compiler and platform, replacing m926's union over candidate series.
- **FR-007**: A component that gains a version MUST record where it came from: the compiler release and the table. It MUST no longer carry C170 `compiler-supplied` or a C151 reason claiming no version could be found. Before adding a field, the plan MUST audit the standards-native constructs and existing `waybill:` rows for a carrier (Principle V), and cite the result.
- **FR-008**: When the compiler is identified but absent from the table, no boot library may gain a version, the compiler-supplied set MUST fall back to m926's rule, and the document MUST record that the compiler was known and had no table entry. That is a different fact from "no compiler identified" (Principle X).
- **FR-009**: When more than one GHC built members of the closure, a boot library MUST take the version of the GHC that built the member depending on it.
  - Where those compilers ship the library at different versions, the document MUST carry one versioned component per version, and each declared edge MUST point at the version its member's compiler shipped.
  - Where they ship the same version, it is one component.
  - A design-tier dependency (declared by the scanned project) takes the version of the compiler that built the scanned output.
- **FR-010**: Versioning a component MUST NOT leave a versionless component of the same library in the document, and MUST NOT orphan any relationship that pointed at the versionless PURL (#980). Versioned components of the same library at different versions (FR-009) are not duplicates.
- **FR-011**: Scans without `--nix-closure`, and scans whose closure identifies no compiler, MUST be unchanged. Their goldens MUST stay byte-identical.
- **FR-012**: No new Cargo dependencies, and no network access at scan time. The table ships with waybill. Only the probe that produces it reaches the network.
- **FR-013**: The behaviour MUST be testable without network access or real builds, using the existing stub closures and fixtures: compiler known and in the table; known and absent; two compilers that ship a library at the same version, giving one component; two that ship different versions, giving two components with correctly split edges; a platform-specific library.

### Key Entities

- **Boot-library table**: a committed, dated mapping from a GHC release `X.Y.Z` to `{library: version}`. It records, per library, its source and any platform restriction, and it is produced by the probe.
- **Closure compiler**: the GHC derivation that built a closure member. Its version is the table key. A closure may hold more than one (the project's and a bootstrap), and each contributes its own versioned boot-library components (FR-009).
- **Compiler-supplied set**: the libraries treated as shipped by the compiler. When the closure compiler is known and in the table, it is the table's entries for it; otherwise it is m926's rule.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the five Haskell closure goldens, of today's versionless `pkg:hackage` components, every boot library the compiler ships carries a version: moat 24 of 24, openai 25 of 26, shellcheck 26 of 26, slack-web 26 of 27, slacklinker 27 of 29.
  - The remainder (`os-string` on openai; `hspec-discover`; `hunit` and `slack-web` on slacklinker) are not shipped by that compiler, and none of them gains a version from the table.
  - Any component added by FR-009 for the bootstrap compiler is versioned too, so no `compiler-supplied` component remains versionless where its compiler is in the table.
- **SC-002**: osv-scanner run on each of those five documents reports HSEC-2023-0007 against `base`. Today it reports it against none.
- **SC-003**: On openai, `os-string` is no longer `compiler-supplied`.
- **SC-004**: Every committed golden of a scan without `--nix-closure` is byte-identical before and after (FR-011).
- **SC-005**: The table check (FR-004) passes against every GHC package database available to it, and is shown to fail when one table entry is altered.
- **SC-006**: Every versioned boot-library component names the compiler release its version came from, in all three formats with parity.

## Assumptions

- **Release notes are the primary source.** They are GHC's own statement of what each release ships. They matched GHC's release tag on the one disputed entry, where Stack did not. Stack fills only what the notes omit, because it has at least one known error.
- **The compiler is the one the closure names.** A GHC's derivation name carries its release (`ghc-9.10.3`). An overlay that changes a boot library's version without changing the compiler's version is out of scope (Edge Cases).
- **A design-tier component may take a version from build evidence.** The library the project declares is the one the compiler supplied in this build; the closure is the evidence for which compiler that was. This is the same evidence the closure tier already uses for identity (m1275). The component keeps its `design` tier label (clarified 2026-11-12). Its version's origin is carried by FR-007's record, as m926's nixpkgs-derived versions are.
- **Advisory applicability is out of scope.** A true version produces true matches against an advisory's ranges, including platform-specific ones (HSEC-2024-0003). Saying that one does not apply is VEX's job.
- **Out of scope:**
  - the static (m926) tier without a closure;
  - haskell.nix;
  - reading each scan's own GHC package database at scan time. It is more authoritative per artifact, but it costs about 8 s of `nix` when the path isn't local, and its answer would depend on what is installed. FR-004 uses it for verification instead.
