#!/usr/bin/env python3
"""Count pkg-config (.pc) and CMake package-version files in each public-corpus
git target, at its pinned commit, via GitHub's git-trees API (no clone, no scan).

Usage (from the repo root):
  python3 -I -B specs/1531-pkgconfig-cmake-versions/measurements/corpus-pc-cmake.py \
      waybill-cli/tests/corpus_harness_195/manifest.rs > corpus-pc-cmake.tsv
  python3 -I -B .../corpus-pc-cmake.py --control <owner>/<repo> <sha>

Targets and pins are read from the TARGETS table in manifest.rs. OCI-image
targets are reported on stderr and skipped. Requires an authenticated `gh`.
"""
import json
import re
import subprocess
import sys

COLUMNS = ["target", "repo", "sha", "truncated", "pc_total", "pc_in_pkgconfig_dir",
           "cmake_version_total", "cmake_version_in_cmake_dir", "paths"]


def parse_manifest(text):
    start = text.index("pub const TARGETS")
    body = text[start:]
    entries = re.split(r'\n\s*name: "', body)[1:]
    git, images = [], []
    for e in entries:
        name = e.split('"', 1)[0]
        url = re.search(r'clone_url:\s*"([^"]+)"', e)
        img = re.search(r'image_ref:\s*"([^"]+)"', e)
        hexm = re.search(r'hex:\s*"([0-9a-f]{40})"', e)
        if url and hexm:
            repo = re.sub(r"^https://github\.com/", "", url.group(1)).removesuffix(".git")
            git.append((name, repo, hexm.group(1)))
        elif img:
            images.append((name, img.group(1)))
        else:
            raise SystemExit(f"unparsed target entry: {name}")
    return git, images


def fetch_tree(repo, sha):
    out = subprocess.run(["gh", "api", f"repos/{repo}/git/trees/{sha}?recursive=1"],
                         check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def classify(tree):
    blobs = [t["path"] for t in tree.get("tree", []) if t.get("type") == "blob"]
    pc = [p for p in blobs if p.endswith(".pc")]  # excludes *.pc.in by construction
    pc_dir = [p for p in pc if "/" in p and p.rsplit("/", 2)[-2] == "pkgconfig"]
    cm = [p for p in blobs
          if p.endswith("ConfigVersion.cmake") or p.endswith("-config-version.cmake")]
    cm_dir = [p for p in cm
              if any(seg in ("cmake", "CMake") for seg in p.split("/")[:-1])]
    return blobs, pc, pc_dir, cm, cm_dir


def row(name, repo, sha):
    tree = fetch_tree(repo, sha)
    blobs, pc, pc_dir, cm, cm_dir = classify(tree)
    if not blobs:
        raise SystemExit(f"{name}: tree has zero blobs — refusing to report zeros")
    return [name, repo, sha, str(bool(tree.get("truncated"))), str(len(pc)), str(len(pc_dir)),
            str(len(cm)), str(len(cm_dir)), ";".join(pc + cm)], len(blobs)


def main(argv):
    print("\t".join(COLUMNS))
    if argv[:1] == ["--control"]:
        r, n = row("CONTROL", argv[1], argv[2])
        print("\t".join(r))
        print(f"CONTROL {argv[1]}@{argv[2]}: {n} blobs", file=sys.stderr)
        return
    git, images = parse_manifest(open(argv[0], encoding="utf-8").read())
    for name, ref in images:
        print(f"IMAGE (skipped): {name} {ref}", file=sys.stderr)
    for name, repo, sha in git:
        r, n = row(name, repo, sha)
        print("\t".join(r))
        print(f"{name}: {n} blobs, truncated={r[3]}", file=sys.stderr)
    print(f"git targets: {len(git)}, image targets: {len(images)}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1:])
