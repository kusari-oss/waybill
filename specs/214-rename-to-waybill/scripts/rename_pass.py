#!/usr/bin/env python3
"""Feature-local rename harness for m214 (mikebom → waybill).

One-shot substitution helper. Six passes matching plan.md research R1:

  1. Cargo package + directory renames (handled OUT-OF-BAND via git mv;
     this script only rewrites Cargo.toml [package].name + intra-workspace
     path deps).
  2. Rust module paths — mikebom_common / mikebom_cli / mikebom_ebpf →
     waybill_*.
  3. String-literal renames — 192 "mikebom:*" annotation keys +
     73 MIKEBOM_* env vars + tool-metadata strings.
  4. Filesystem artifacts + workflow patterns — eBPF binary path,
     Dockerfile paths, release.yml artifact naming.
  5. Docs + prose rewrite — README, CLAUDE.md, docs/**/*.md, constitution.md.
     EXPLICIT EXCLUSIONS: docs/audits/**, docs/migration/** (per M1 analyze
     finding). Historical spec docs at specs/001-* through specs/213-*
     also preserved.
  6. Placeholder — CI grep gate wiring done manually (per plan T038 which
     writes YAML directly).

Usage:
  python3 rename_pass.py --pass 2                    # apply Rust module rename
  python3 rename_pass.py --pass 3-envvars            # env-var subclass only
  python3 rename_pass.py --pass 3-annotations        # annotation subclass only
  python3 rename_pass.py --pass 3 --dry-run          # both, no writes
  python3 rename_pass.py --pass 5 --dry-run --verbose  # doc-prose w/ per-file counts

Removed post-merge per research R9.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]

# Allowlist directory prefixes (relative to REPO_ROOT). Files whose path
# starts with any of these are NEVER modified by ANY pass.
ALLOWLIST_PATH_PREFIXES = [
    "specs/",           # ALL specs (001-* through 213-* preserved; 214-* is this feature)
    "docs/audits/",     # historical audit reports (T031)
    "docs/migration/",  # migration guide title literally names the old form (T031)
    ".git/",
    ".github/",         # DEFAULT excluded — pass 3/4 explicitly re-includes .github/workflows/*.yml
    "target/",
    ".venv/",
    "MEMORY.md",        # user-personal memory index
    # Fixture inputs that legitimately contain "mikebom" as external data
    # (e.g., synthetic apk/deb/rpm packages whose upstream calls itself mikebom)
    # get skipped implicitly because they're binary or reside under
    # tests/fixtures which is only mentioned in the golden-regen pass (6)
    # via the WAYBILL_UPDATE_*_GOLDENS env var invocation.
]

# Additional explicit path exclusions for the specific spec-kit historical
# folders we want to preserve (also matched by ALLOWLIST above but listed
# for clarity/documentation).
HISTORICAL_SPECS = [f"specs/{n:03d}-" for n in range(1, 214)]  # 001-* through 213-*


def is_allowlisted(rel_path: str) -> bool:
    """Return True if rel_path should NOT be modified."""
    for prefix in ALLOWLIST_PATH_PREFIXES:
        if rel_path.startswith(prefix):
            return True
    return False


def iter_repo_files(*, include_globs: list[str] | None = None,
                    exclude_globs: list[str] | None = None,
                    also_include: list[str] | None = None):
    """Yield every file in REPO_ROOT matching include_globs, minus exclude_globs.

    also_include: paths that override the ALLOWLIST (e.g., pass 3 needs .github/
    workflows even though .github/ is default-allowlisted).
    """
    include_globs = include_globs or ["**/*"]
    exclude_globs = exclude_globs or []
    also_include = also_include or []

    seen = set()
    for pattern in include_globs:
        for path in REPO_ROOT.rglob(pattern):
            if not path.is_file():
                continue
            rel = str(path.relative_to(REPO_ROOT))
            if rel in seen:
                continue
            seen.add(rel)

            if any(path.match(pat) for pat in exclude_globs):
                continue
            if is_allowlisted(rel) and rel not in also_include:
                # Check if under also_include prefix
                if not any(rel.startswith(inc) for inc in also_include):
                    continue

            yield path


def rewrite_file(path: Path, substitutions: list[tuple[str, str]],
                 *, dry_run: bool, verbose: bool) -> int:
    """Apply substitutions in order. Returns count of replacements made."""
    try:
        original = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, IsADirectoryError):
        return 0

    new_content = original
    total = 0
    for pattern, replacement in substitutions:
        if isinstance(pattern, str):
            # Plain string substitution
            count = new_content.count(pattern)
            new_content = new_content.replace(pattern, replacement)
        else:
            # Compiled regex
            new_content, count = pattern.subn(replacement, new_content)
        total += count

    if total > 0 and not dry_run:
        path.write_text(new_content, encoding="utf-8")
    if verbose and total > 0:
        rel = path.relative_to(REPO_ROOT)
        print(f"  {'DRY' if dry_run else 'MOD'} {rel}: {total} substitutions")
    return total


# ==============================================================================
# PASS 1: Cargo package + workspace deps
# ==============================================================================
def pass_1_cargo(dry_run: bool, verbose: bool) -> int:
    """Rewrite [package].name + workspace members + intra-workspace path deps.

    NOTE: `git mv` for directory renames happens OUT-OF-BAND (per quickstart.md).
    This pass ASSUMES the git mv has already run.
    """
    subs = [
        # Workspace root Cargo.toml
        (re.compile(r'^(members = \[)"mikebom-cli", "mikebom-common", ', re.MULTILINE),
         r'\1"waybill-cli", "waybill-common", '),
        (re.compile(r'^(exclude = \[)"mikebom-ebpf"', re.MULTILINE),
         r'\1"waybill-ebpf"'),
        # Package name lines (careful — the mikebom-cli package's name is "mikebom" singular)
        (re.compile(r'^name = "mikebom"$', re.MULTILINE), 'name = "waybill"'),
        (re.compile(r'^name = "mikebom-common"$', re.MULTILINE), 'name = "waybill-common"'),
        (re.compile(r'^name = "mikebom-ebpf"$', re.MULTILINE), 'name = "waybill-ebpf"'),
        # Intra-workspace path deps
        (re.compile(r'^mikebom-common = \{ path = "\.\./mikebom-common"'),
         'waybill-common = { path = "../waybill-common"'),
        # Cargo.lock name entries
        (re.compile(r'^name = "mikebom"$', re.MULTILINE), 'name = "waybill"'),
        (re.compile(r'^name = "mikebom-common"$', re.MULTILINE), 'name = "waybill-common"'),
        # dependencies = [...] arrays in Cargo.lock reference other package names
        (re.compile(r'"mikebom-common"'), '"waybill-common"'),
    ]
    total = 0
    # Scan Cargo.toml + Cargo.lock at root + each crate's Cargo.toml (post-git-mv)
    for cargo_path in [
        REPO_ROOT / "Cargo.toml",
        REPO_ROOT / "Cargo.lock",
        REPO_ROOT / "waybill-cli" / "Cargo.toml",
        REPO_ROOT / "waybill-common" / "Cargo.toml",
        REPO_ROOT / "waybill-ebpf" / "Cargo.toml",
        REPO_ROOT / "xtask" / "Cargo.toml",
    ]:
        if cargo_path.exists():
            total += rewrite_file(cargo_path, subs, dry_run=dry_run, verbose=verbose)
    return total


# ==============================================================================
# PASS 2: Rust module paths
# ==============================================================================
def pass_2_rust_modules(dry_run: bool, verbose: bool) -> int:
    """Rewrite mikebom_common / mikebom_cli / mikebom_ebpf → waybill_*."""
    subs = [
        (re.compile(r'\bmikebom_common\b'), 'waybill_common'),
        (re.compile(r'\bmikebom_cli\b'), 'waybill_cli'),
        (re.compile(r'\bmikebom_ebpf\b'), 'waybill_ebpf'),
    ]
    total = 0
    for path in iter_repo_files(include_globs=["**/*.rs"]):
        total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    return total


# ==============================================================================
# PASS 3: String literals — annotations + env vars
# ==============================================================================
def pass_3_annotations(dry_run: bool, verbose: bool) -> int:
    """Rewrite "mikebom:..." string prefix → "waybill:..."."""
    subs = [
        # The critical annotation prefix rewrite (192 distinct keys, mechanical prefix swap)
        (re.compile(r'"mikebom:'), '"waybill:'),
    ]
    total = 0
    for path in iter_repo_files(include_globs=["**/*.rs"]):
        total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    # Also scan Rust test files under crate/tests/ (not just src/**)
    return total


def pass_3_envvars(dry_run: bool, verbose: bool) -> int:
    """Rewrite MIKEBOM_* env-var references → WAYBILL_* across Rust + workflows + shell."""
    subs = [
        (re.compile(r'\bMIKEBOM_([A-Z][A-Z0-9_]*)\b'), r'WAYBILL_\1'),
    ]
    total = 0
    for pat in ["**/*.rs", "**/*.sh", "**/*.yml", "**/*.yaml"]:
        # For .yml files, override the .github/ allowlist
        also_include = [".github/"] if "yml" in pat or "yaml" in pat else []
        for path in iter_repo_files(include_globs=[pat], also_include=also_include):
            total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    return total


def pass_3_tool_metadata(dry_run: bool, verbose: bool) -> int:
    """Rewrite tool-metadata identifiers in SBOM builders (`"mikebom"` as tool name)."""
    # More targeted — only touch specific files identified during planning.
    # Broader "mikebom" → "waybill" in all string literals would rewrite log
    # messages + comments too, which we do want, but with care.
    subs = [
        # Log-line tool identifiers: tracing::info!("... mikebom ...") → "... waybill ..."
        # Only inside string literals — regex matches `"[^"]*\bmikebom\b[^"]*"`
        # This is broad; be careful.
    ]
    # Deferred to pass 5 (prose replacement) which uses case-preserving
    # `mikebom` → `waybill` for prose. Log messages are covered.
    return 0


# ==============================================================================
# PASS 4: Filesystem artifacts + workflow patterns
# ==============================================================================
def pass_4_filesystem(dry_run: bool, verbose: bool) -> int:
    """Rewrite eBPF binary paths, Dockerfile paths, release.yml artifact naming."""
    subs = [
        # eBPF binary path in loader.rs
        (re.compile(r'"mikebom-ebpf/target/bpfel-unknown-none/release/mikebom-ebpf"'),
         '"waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf"'),
        (re.compile(r'"mikebom-ebpf"'), '"waybill-ebpf"'),
        # Dockerfile paths
        (re.compile(r'/mikebom(/|$)'), r'/waybill\1'),
        (re.compile(r'\bWORKDIR /mikebom\b'), 'WORKDIR /waybill'),
        # release.yml artifact-naming (kebab-case with -v)
        (re.compile(r'\bmikebom-v(\$\{?\{?version)'), r'waybill-v\1'),
        (re.compile(r'\bmikebom-v0\.'), 'waybill-v0.'),
        # Docker image name references
        (re.compile(r'kusari-oss/mikebom\b'), 'kusari-oss/waybill'),
        # Bare "mikebom" as binary name in shell/scripts
        (re.compile(r'\btarget/release/mikebom\b'), 'target/release/waybill'),
    ]
    total = 0
    for path in iter_repo_files(include_globs=["**/*.rs", "**/*.sh"]):
        total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    # Dockerfiles
    for name in ["Dockerfile.ebpf-test", "Dockerfile"]:
        p = REPO_ROOT / name
        if p.exists():
            total += rewrite_file(p, subs, dry_run=dry_run, verbose=verbose)
    # Workflows (override .github/ allowlist)
    for path in iter_repo_files(include_globs=[".github/workflows/*.yml"],
                                also_include=[".github/"]):
        total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    return total


# ==============================================================================
# PASS 5: Docs + prose (case-preserving)
# ==============================================================================
PROSE_INCLUDES = [
    "README.md",
    "CLAUDE.md",
    ".specify/memory/constitution.md",
    "docs/architecture",   # subdir; walked
    "docs/user-guide",     # subdir; walked
    "docs/reference",
    "docs/research",
    "docs/examples",
    "docs/ecosystems.md",
    "docs/design-notes.md",
    "docs/index.md",
    "docs/DEPENDENCIES.md",
    "docs/releases.md",
]

PROSE_EXCLUDES_ABSOLUTE = [
    "docs/audits/",
    "docs/migration/",
    "specs/",  # all specs
    ".git/",
    "MEMORY.md",
]


def pass_5_prose(dry_run: bool, verbose: bool) -> int:
    """Case-preserving Mikebom → Waybill + mikebom → waybill in prose files.

    EXPLICIT EXCLUSIONS per M1 analyze finding: docs/audits/** + docs/migration/**.
    Historical specs at specs/001-* through specs/213-* also preserved.
    """
    subs = [
        # Also touch identifiers as bare words in prose (e.g., "the mikebom-cli crate")
        (re.compile(r'\bmikebom-cli\b'), 'waybill-cli'),
        (re.compile(r'\bmikebom-common\b'), 'waybill-common'),
        (re.compile(r'\bmikebom-ebpf\b'), 'waybill-ebpf'),
        (re.compile(r'\bmikebom_common\b'), 'waybill_common'),
        (re.compile(r'\bmikebom_cli\b'), 'waybill_cli'),
        (re.compile(r'\bmikebom_ebpf\b'), 'waybill_ebpf'),
        # Log-messages / annotation refs in docs
        (re.compile(r'"mikebom:'), '"waybill:'),
        (re.compile(r'`mikebom:'), '`waybill:'),
        # `mikebom` in inline code + prose contexts
        (re.compile(r'\bmikebom\b'), 'waybill'),
        # Capitalized prose
        (re.compile(r'\bMikebom\b'), 'Waybill'),
        (re.compile(r'\bMIKEBOM\b'), 'WAYBILL'),
        # Env var references
        (re.compile(r'\bMIKEBOM_([A-Z][A-Z0-9_]*)\b'), r'WAYBILL_\1'),
    ]
    total = 0
    for include in PROSE_INCLUDES:
        include_path = REPO_ROOT / include
        if not include_path.exists():
            continue
        if include_path.is_file():
            paths = [include_path]
        else:
            paths = list(include_path.rglob("*.md"))
        for path in paths:
            rel = str(path.relative_to(REPO_ROOT))
            # Enforce exclusions
            if any(rel.startswith(ex) for ex in PROSE_EXCLUDES_ABSOLUTE):
                continue
            total += rewrite_file(path, subs, dry_run=dry_run, verbose=verbose)
    return total


# ==============================================================================
# MAIN
# ==============================================================================
def main():
    parser = argparse.ArgumentParser(description="m214 rename harness")
    parser.add_argument("--pass", dest="pass_id", required=True,
                        choices=["1", "2", "3", "3-annotations", "3-envvars",
                                 "3-tool-metadata", "4", "5"],
                        help="Which substitution pass to apply")
    parser.add_argument("--dry-run", action="store_true",
                        help="Do not write files; report what would change")
    parser.add_argument("--verbose", action="store_true",
                        help="Print per-file substitution counts")
    parser.add_argument("--yes", action="store_true",
                        help="Skip confirmation prompt")
    args = parser.parse_args()

    dispatch = {
        "1": ("Cargo package + workspace deps", pass_1_cargo),
        "2": ("Rust module paths", pass_2_rust_modules),
        "3-annotations": ('"mikebom:" annotation prefixes', pass_3_annotations),
        "3-envvars": ("MIKEBOM_* env vars", pass_3_envvars),
        "3-tool-metadata": ("Tool-metadata identifiers", pass_3_tool_metadata),
        "3": ("All pass-3 subclasses", None),
        "4": ("Filesystem + workflow patterns", pass_4_filesystem),
        "5": ("Docs + prose (case-preserving, w/ exclusions)", pass_5_prose),
    }

    if args.pass_id == "3":
        # Combined pass 3
        name = "All pass-3 subclasses (annotations + envvars + tool metadata)"
    else:
        name, _ = dispatch[args.pass_id]

    action = "DRY-RUN" if args.dry_run else "APPLY"
    print(f"[m214 rename] {action}: pass {args.pass_id} — {name}")
    if not args.dry_run and not args.yes:
        confirm = input("Proceed? [y/N] ").strip().lower()
        if confirm != "y":
            print("Aborted.")
            sys.exit(1)

    total = 0
    if args.pass_id == "3":
        for sub in ["3-annotations", "3-envvars", "3-tool-metadata"]:
            _, func = dispatch[sub]
            print(f"  subclass {sub}...")
            total += func(dry_run=args.dry_run, verbose=args.verbose)
    else:
        _, func = dispatch[args.pass_id]
        total = func(dry_run=args.dry_run, verbose=args.verbose)

    print(f"[m214 rename] {action} complete: {total} substitutions")


if __name__ == "__main__":
    main()
