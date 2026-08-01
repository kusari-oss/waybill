# Feature Specification: Pants pex-lockfile reader

**Feature Branch**: `223-pants-pex-reader`
**Created**: 2026-07-31
**Status**: Draft (post-clarify — 2 questions resolved 2026-07-31)
**Input**: User description: "pants-pex-lockfile-reader"

## Clarifications

### Session 2026-07-31

- Q: Multi-resolve scope — when a Pants repo has multiple named resolves (default + mypy + pytest), how do we handle non-default resolves? → A: Scan every resolve. Components from the `default` resolve tag as `lifecycle_scope=Runtime` (unchanged from today's pip reader default). Components from resolves whose name matches a known dev-tool set (`mypy`, `pytest`, `flake8`, `black`, `ruff`, `isort`, `bandit`, `coverage`, `lint`, `test`, `dev`, `ci`) tag as `lifecycle_scope=Dev`. Every component gets a `waybill:pants-resolve=<name>` annotation for auditability. Unknown resolve names default to `Runtime` with the annotation preserved (operators can spot-check and re-tag downstream).
- Q: Non-PyPI lockfile entries (git URLs, direct download URLs, local paths) — how do we represent these when there's no clean `pkg:pypi/*` PURL? → A: Follow waybill's existing `pkg:generic/*` convention. PyPI-hosted entries stay `pkg:pypi/<name>@<version>`; git-URL / direct-URL / local-path entries emit `pkg:generic/<name>@<version>` plus `waybill:source-url=<...>` + `waybill:source-type={git,url,local}` annotations. Keeps vuln-scanner PURL semantics honest (they won't pivot to a fake PyPI CVE lookup on a git-sourced package) and avoids adding new `waybill:*` annotations that need parity-extractor rows.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Scan a Pants Python repo and get Python components in the SBOM (Priority: P1)

A platform-security operator runs `waybill sbom scan` against a source
tree that uses the Pants build system for its Python targets. Today,
the resulting SBOM lists zero Python components — Pants stores its
resolved dependency graph in a Pex lockfile format (typically at
`3rdparty/python/*.lock` or a path configured in `pants.toml`) that
waybill's existing Python readers (pip / poetry / uv) do not
recognize. The operator wants those Python packages to appear in the
SBOM with the same fidelity as any pip-managed repo — component
names, versions, `pkg:pypi/*` PURLs, artifact hashes, licenses where
available, and a dependency graph if the lockfile carries one.

**Why this priority**: This is the entire reason the feature exists.
Without it, Pants-Python repos are invisible to waybill's Python
coverage. Pants is not a mass-market build system, but the orgs that
use it (Toolchain, IBM, several enterprise Python monorepos) are the
kind of operator waybill's compliance angle matters most to. Shipping
this closes a "we can't scan our own repo" gap for those users.

**Independent Test**: Point `waybill sbom scan` at a Pants Python
project fixture that contains a valid Pex lockfile. Assert the
emitted CDX contains one `pkg:pypi/<name>@<version>` component per
locked distribution, with matching artifact hashes and (where the
lockfile records them) license strings. Compare against Trivy /
Syft output on the same fixture — waybill's counts should match or
exceed.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `3rdparty/python/default.lock` containing 20 locked Python distributions, **When** the operator runs `waybill sbom scan --path <repo>`, **Then** the emitted SBOM contains 20 Python components (one per locked distribution) with `pkg:pypi/<name>@<version>` PURLs and the sha256 artifact hashes recorded by the lockfile.
2. **Given** the same repo, **When** the SBOM is emitted, **Then** each locked component's dependency edges reflect the `requires` array in the Pex lockfile entry (if the lockfile records inter-package dependencies for its resolve).
3. **Given** a Pants repo where the lockfile is at a non-default path (declared in `pants.toml` under `[python].lockfile`), **When** the operator runs the scan, **Then** waybill discovers the lockfile at the configured path (not just the default) and produces the same output as scenario 1.
4. **Given** a Pants repo with **multiple named resolves** (default + `mypy` + `pytest`, each with its own lockfile), **When** the scan runs, **Then** every resolve is scanned. Default-resolve components tag `lifecycle_scope=Runtime`; components from resolves named after known dev tools (`mypy`, `pytest`, `flake8`, `black`, `ruff`, `isort`, `bandit`, `coverage`, `lint`, `test`, `dev`, `ci`) tag `lifecycle_scope=Dev`. Every component carries a `waybill:pants-resolve=<name>` annotation identifying the source lockfile. Unknown resolve names default to `Runtime` with the annotation preserved.

---

### User Story 2 — Correct duplicate handling when a Pants repo also has a `requirements.txt` (Priority: P2)

Some Pants repos carry a `requirements.txt` alongside the Pex lockfile
— either for compatibility with non-Pants tooling (IDEs, Dependabot,
`pip freeze` exports), or because the lockfile was generated FROM the
requirements file. If waybill's pip reader finds the requirements
file and waybill's new pants-pex reader finds the lockfile, the same
Python packages get discovered twice. The operator wants the SBOM to
list each package exactly once, without random selection about
"which reader wins."

**Why this priority**: Broken deduplication is a very visible
correctness problem — operators comparing waybill output to their
own dependency inventory notice extra components immediately, and it
undermines trust in the whole SBOM.

**Independent Test**: Craft a fixture with both `3rdparty/python/default.lock`
and `requirements.txt` naming overlapping packages. Assert the SBOM
lists each Python package exactly once, with the more authoritative
source (the lockfile — has hashes and pinned versions) winning.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `3rdparty/python/default.lock` AND `requirements.txt` both listing `requests==2.31.0`, **When** the scan runs, **Then** the SBOM contains exactly ONE `pkg:pypi/requests@2.31.0` component, sourced from the lockfile (which carries the sha256 hash the requirements file lacks).
2. **Given** the same repo, **When** the SBOM is emitted, **Then** the component's `waybill:also-detected-via` annotation (or equivalent existing dedup annotation from milestone 105+) records that the requirements.txt was also observed, so operators can audit the dedup decision.

---

### User Story 3 — `pants.toml` discovery drives which lockfiles get scanned (Priority: P3)

Pants supports arbitrary lockfile paths via configuration. Rather than
hardcoding `3rdparty/python/*.lock`, waybill reads `pants.toml` to
learn which paths the operator's Pants config points at, and scans
those exact paths. This matters because:
- Some repos put lockfiles at `build-support/python.lock` or similar
- Multi-resolve repos configure per-resolve paths explicitly
- Missing this config causes false-negative "we didn't find any
  Python packages" for repos with a valid but non-standard layout

**Why this priority**: Nice-to-have for the "config-driven layouts"
long-tail; the default `3rdparty/python/*.lock` glob covers the
majority of Pants repos out of the box. Deferrable if the P1 scope
ships with just the default glob.

**Independent Test**: Create a fixture with `pants.toml` declaring
`[python].lockfile = "build-support/py.lock"` and NO file at
`3rdparty/python/`. Assert the scan finds the packages in
`build-support/py.lock`.

**Acceptance Scenarios**:

1. **Given** `pants.toml` declares `[python].lockfile = "build-support/py.lock"`, **When** the scan runs, **Then** waybill discovers packages in `build-support/py.lock` (NOT `3rdparty/python/default.lock`, which doesn't exist).
2. **Given** `pants.toml` is missing or invalid, **When** the scan runs, **Then** waybill falls back to the default `3rdparty/python/*.lock` glob and continues normally (no hard failure).

---

### Edge Cases

- **Empty or partially-generated lockfile**: `pants generate-lockfiles` was interrupted, leaving a file with header + zero entries. waybill emits an INFO diagnostic and produces zero components from that lockfile (no components ≠ scan failure).
- **Lockfile with entries that lack version pins** (uncommon but possible with `--allow-prereleases` or unpinned inputs): waybill emits the component with a placeholder version marker (matching how the pip reader handles the same case) and an annotation indicating unpinned resolution.
- **Corrupted / non-JSON-parseable lockfile bytes**: waybill emits a WARN, skips that lockfile, and continues scanning the rest of the repo (no scan-abort).
- **Lockfile schema version mismatch**: Pex lockfile format has a version field (`"pex_version"` / `"lock_version"`). If waybill sees a version it doesn't support, it emits a WARN naming the unsupported version and skips that lockfile, but still processes any others.
- **Non-PyPI locks** (Pex supports git URLs, local paths, direct download URLs as "artifact" entries): PyPI entries emit `pkg:pypi/<name>@<version>` normally. Non-PyPI entries emit `pkg:generic/<name>@<version>` with `waybill:source-url=<url-or-path>` + `waybill:source-type={git,url,local}` annotations. Vuln scanners that pivot on PURL will not falsely look up PyPI CVEs for git-sourced packages, and the source-url annotation preserves the audit trail.
- **A `pants.toml` exists but declares no Python backend** (Pants is used only for JVM or Go in this repo): waybill emits an INFO log noting "Pants config detected, no Python resolves configured", scans zero Pex lockfiles, exits cleanly. Downstream Go / JVM readers still run normally.
- **Pants is present but no lockfile exists yet** (e.g., a fresh checkout before `pants generate-lockfiles` has run): waybill emits an INFO log and produces no Python components from Pants (the pip reader may still find `requirements.txt` or `pyproject.toml` if present).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST discover Pex lockfiles by default at the glob `3rdparty/python/*.lock` relative to the scan root. Additional paths from `pants.toml` config are covered by FR-004.
- **FR-002**: For each locked Python distribution in a Pex lockfile, waybill MUST emit exactly one SBOM component with:
  - purl of form `pkg:pypi/<name>@<version>` (lowercase name per purl-spec's pypi type)
  - the sha256 artifact hash recorded in the lockfile (in the component's `hashes[]` per CDX and equivalent SPDX slots)
  - NO license string extracted at reader time — Pex lockfile format does not carry PyPI Trove-classifier license metadata (verified empirically in research.md §R1). Licenses come from downstream enrichment (deps.dev / ClearlyDefined) if enabled, matching the existing pip reader's behavior.
  - `sbom_tier: "source"` (lockfile-derived, matching how the existing pip reader tags `requirements.txt`-sourced components)
- **FR-003**: When the lockfile records inter-package dependencies (`requires` array on lock entries), waybill MUST emit those as SBOM `dependsOn` relationships between the corresponding components.
- **FR-004**: waybill MUST read `pants.toml` (if present at the scan root) and honor the `[python].lockfile` config (or equivalent per-resolve path config) to discover lockfiles at non-default paths. If `pants.toml` is absent, missing the relevant key, or unparseable, waybill MUST fall back to the FR-001 default glob without failing.
- **FR-005**: When both a Pex lockfile and a competing Python manifest (`requirements.txt`, `pyproject.toml`, `poetry.lock`, `uv.lock`) list the same PURL, waybill MUST emit exactly ONE component. The lockfile source wins because it carries authoritative artifact hashes; the losing source is recorded via the existing `waybill:also-detected-via` annotation channel (milestone 105+) so operators can audit the decision.
- **FR-006**: waybill MUST NOT abort the scan if a Pex lockfile is corrupt, unparseable, or has an unsupported schema version. It MUST emit a WARN diagnostic naming the file + reason, skip that specific lockfile, and continue processing the rest of the repo.
- **FR-007**: The default emit path (no Pex lockfiles present in the scanned repo) MUST be byte-identical to today's goldens — the new reader activates only when a Pex lockfile is discovered.
- **FR-008**: When a Pants repo declares multiple named resolves, waybill MUST scan every discovered resolve (whether at `3rdparty/python/<name>.lock` or a `pants.toml`-configured path). Components from the `default` resolve MUST tag `lifecycle_scope=Runtime`; components from resolves whose name is in the known-dev-tool allowlist (`mypy`, `pytest`, `flake8`, `black`, `ruff`, `isort`, `bandit`, `coverage`, `lint`, `test`, `dev`, `ci`) MUST tag `lifecycle_scope=Dev`. Unknown resolve names default to `Runtime`. Every emitted component MUST carry a `waybill:pants-resolve=<name>` annotation identifying its source lockfile.
- **FR-009**: For lockfile entries whose artifact source is PyPI (URL matches `https://files.pythonhosted.org/*` or the entry's `project_name` + `version` resolve on PyPI per the standard purl-spec rules), waybill MUST emit `pkg:pypi/<name>@<version>`. For entries with git-URL, direct-URL, or local-path artifact sources, waybill MUST emit `pkg:generic/<name>@<version>` with two annotations: `waybill:source-url=<url-or-absolute-path>` and `waybill:source-type={git,url,local}`. Local-path entries whose path is inside the scanned repo MUST NOT leak absolute host paths — waybill records the path relative to the scan root.
- **FR-010**: waybill MUST emit an INFO log line at scan-end reporting the number of Pex lockfiles discovered + the number of Python components extracted, so operators can distinguish "Pants config detected, no lockfiles found" from "Pants config detected, lockfile empty" from "Pants config detected, 47 components extracted."

### Key Entities

- **Pex lockfile**: A JSON-shaped file at `3rdparty/python/*.lock` (default) or a path declared in `pants.toml`. Records the resolved Python dependency graph for one named resolve: locked distribution names, versions, sha256 hashes, artifact URLs, per-platform tags, and (optionally) inter-package `requires` edges. Multiple lockfiles can coexist in one repo (one per resolve — `default`, `mypy`, `pytest`, etc.).
- **Pants config file (`pants.toml`)**: TOML file at the repo root declaring which Pants backends are enabled (Python, JVM, Go, etc.) and where per-resolve lockfiles live. waybill only cares about the `[python]` section for this feature; other sections are ignored.
- **Named resolve**: A logical grouping in Pants of "these Python targets share this lockfile." Every Pants Python repo has a `default` resolve; some declare additional resolves (e.g., `mypy`, `pytest`, `ci`) for tools that need different Python dep sets. Each resolve maps to exactly one lockfile.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a Pants Python fixture with N locked distributions, waybill emits N Python components in the CDX and N packages in the SPDX 2.3 output (100% coverage of the lockfile's contents, zero fabricated components).
- **SC-002**: For a fixture where both a Pex lockfile and a `requirements.txt` list overlapping packages, the SBOM lists each package exactly once (zero duplicates), with the lockfile as the recorded source.
- **SC-003**: On a repo that does NOT use Pants (no `pants.toml`, no `3rdparty/python/*.lock`), scan runtime and SBOM output are byte-identical to pre-feature-223 goldens (feature adds zero cost when unused).
- **SC-004**: For a Pants fixture with a lockfile carrying inter-package `requires` edges, the SBOM's `dependsOn` graph matches the lockfile's declared graph (spot-check: the top 3 dependency edges observable in the lockfile appear identically in the SBOM).
- **SC-005**: For a repo with a corrupted Pex lockfile, the scan exits cleanly (exit code 0), emits a WARN naming the file, and still produces components from any other Pex lockfiles or Python manifests in the same repo (fail-open on per-file corruption, no scan-abort).
- **SC-006** (post-ship acceptance, not a merge gate): Waybill's Pex-lockfile-derived component count for a real-world Pants repo (e.g., Pants's own dogfood tree at `github.com/pantsbuild/pants`) is within ±5% of what Syft (v1.19+, has explicit `python-pex-cataloger`) reports when scanning the same tree. Verified manually within one week of feature release; findings recorded as a note in `docs/audits/` per the m165/m168 audit convention. Trivy does not currently support pex lockfiles as of v0.71.1 (per research.md §R5 prior-art check); waybill's absolute count is the ground truth for Trivy comparison.

## Assumptions

- **Pex lockfile format**: the JSON shape produced by `pants generate-lockfiles` (Pex ≥ 2.1). Older Pex 1.x plaintext lockfiles are out of scope for v1 (Pants 2.x uses Pex ≥ 2.1 exclusively).
- **Pants version target**: Pants 2.x (current major). Pants 1.x is a distinct build system with a very different config surface and is out of scope.
- **`pants.toml` parse depth**: waybill parses only the `[python].lockfile` key (plus per-resolve equivalents where applicable). Full Pants config is intentionally NOT parsed — that would couple us to Pants's own config schema evolution.
- **License data**: extracted opportunistically from the Pex lockfile's PyPI metadata cache when the lockfile records it. Absent-license is not a failure — matches how the pip reader handles the same gap.
- **No Pants binary invocation**: waybill parses the lockfile bytes on disk, does not shell out to `pants` for any part of this feature. Matches waybill's "no build-tool subprocess for read-time discovery" posture (e.g., existing cargo / npm readers don't invoke their build tools either).
- **Multi-tier scope**: this feature is source-tier only (lockfile bytes = source of truth). Design-tier (BUILD file parsing) and deployed-tier (runtime Python venv inspection) are separate potential follow-ups, out of scope.
- **Test fixture policy**: fixtures use synthetic package names per the `feedback_fixture_synthetic_package_names` project convention — never real PyPI coordinates, to avoid Kusari Inspector advisory-scan noise.
- **Interoperability with existing pip reader**: coexistence via the m191 reconciler's PURL-level dedup path. FR-005 covers the specific dedup rule; the reconciler infrastructure is already in place.
- **Fingerprinting**: this feature does NOT extend waybill's fingerprint corpus (m108+). Pants lockfiles are metadata, not binaries.
- **eBPF integration**: this feature is user-space only; `waybill-ebpf` is untouched. Runtime tracing of Pex/Pants invocations is a separate potential feature (option D from the pre-spec investigation).
- **Constitution Principle I**: no C-native transitive deps needed — Pex lockfiles are JSON, parseable by existing `serde_json` (workspace dep). No new Cargo dependencies expected.
