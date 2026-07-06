#!/usr/bin/env python3
"""Milestone 165 audit analysis — per-tool metrics + failure-mode
classification + tool-comparison delta for one target's SBOM triple
(mikebom.cdx.json, trivy.cdx.json, syft.cdx.json).

Emits `analysis.json` on stdout structured per data-model.md E2/E3/E4.

Usage:
    python3 analyze.py \\
        --target-name <kubernetes|argocd> \\
        --sboms-dir  <path>              \\
        --commit-sha <40-char-hex>       \\
        > <sboms-dir>/analysis.json

Stdlib-only. Python 3.10+.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import sys
from typing import Iterable


EMPTY_VERSION_PURL_RE = re.compile(r"^pkg:(npm|golang)/[^@]+@$")


def load_sbom(path: pathlib.Path) -> dict:
    if not path.is_file():
        return {"components": [], "dependencies": [], "metadata": {}}
    with path.open() as f:
        return json.load(f)


def components_of(sbom: dict) -> list[dict]:
    return sbom.get("components") or []


def purl_of(comp: dict) -> str:
    return comp.get("purl") or ""


def npm_or_golang_purls(sbom: dict) -> list[str]:
    return [
        purl_of(c)
        for c in components_of(sbom)
        if purl_of(c).startswith(("pkg:npm/", "pkg:golang/"))
    ]


def ecosystem_of(purl: str) -> str:
    if purl.startswith("pkg:npm/"):
        return "npm"
    if purl.startswith("pkg:golang/"):
        return "golang"
    if purl.startswith("pkg:"):
        return "other"
    return "other"


def ecosystem_breakdown(sbom: dict) -> dict[str, int]:
    counter: collections.Counter = collections.Counter()
    for c in components_of(sbom):
        counter[ecosystem_of(purl_of(c))] += 1
    return dict(counter)


def edge_count(sbom: dict) -> int:
    deps = sbom.get("dependencies") or []
    return sum(len((d.get("dependsOn") or [])) for d in deps)


def all_edge_targets(sbom: dict) -> Iterable[str]:
    for d in sbom.get("dependencies") or []:
        for t in d.get("dependsOn") or []:
            if isinstance(t, str):
                yield t


def bfs_reachable(sbom: dict) -> tuple[int, int]:
    """Return (reachable, total_npm_or_golang) from metadata.component."""
    root = (sbom.get("metadata") or {}).get("component") or {}
    root_purl = root.get("purl")
    all_purls = set(purl_of(c) for c in components_of(sbom) if purl_of(c))
    tracked = {p for p in all_purls if p.startswith(("pkg:npm/", "pkg:golang/"))}
    if not root_purl:
        return (0, len(tracked))
    adj: dict[str, list[str]] = {}
    for d in sbom.get("dependencies") or []:
        ref = d.get("ref")
        if isinstance(ref, str):
            adj[ref] = list(d.get("dependsOn") or [])
    visited: set[str] = set()
    q: collections.deque[str] = collections.deque([root_purl])
    while q:
        cur = q.popleft()
        if cur in visited:
            continue
        visited.add(cur)
        for t in adj.get(cur, []):
            if t not in visited:
                q.append(t)
    reachable = sum(1 for p in tracked if p in visited)
    return (reachable, len(tracked))


def empty_version_purl_count(sbom: dict) -> int:
    return sum(
        1
        for c in components_of(sbom)
        if EMPTY_VERSION_PURL_RE.match(purl_of(c))
    )


def phantom_edge_count(sbom: dict) -> int:
    return sum(1 for t in all_edge_targets(sbom) if EMPTY_VERSION_PURL_RE.match(t))


def strip_version(purl: str) -> str:
    # "pkg:ecosystem/name@version" → "pkg:ecosystem/name"
    body = purl.rsplit("@", 1)[0]
    return body


def classify_mikebom_orphans(sbom: dict) -> dict:
    """Bucket mikebom's orphans by named root causes per research §R6."""
    root = (sbom.get("metadata") or {}).get("component") or {}
    root_purl = root.get("purl")
    tracked_purls = [
        purl_of(c)
        for c in components_of(sbom)
        if purl_of(c).startswith(("pkg:npm/", "pkg:golang/"))
    ]
    if not root_purl:
        return {"orphans_total": 0, "buckets": {}, "example_by_bucket": {}}

    adj: dict[str, list[str]] = {}
    incoming: dict[str, list[str]] = collections.defaultdict(list)
    for d in sbom.get("dependencies") or []:
        ref = d.get("ref")
        if isinstance(ref, str):
            targets = list(d.get("dependsOn") or [])
            adj[ref] = targets
            for t in targets:
                incoming[t].append(ref)

    visited: set[str] = set()
    q: collections.deque[str] = collections.deque([root_purl])
    while q:
        cur = q.popleft()
        if cur in visited:
            continue
        visited.add(cur)
        for t in adj.get(cur, []):
            if t not in visited:
                q.append(t)

    orphans = [p for p in tracked_purls if p not in visited]

    # Multi-version cluster: same base-name, at least one sibling reachable
    all_by_name: dict[str, list[str]] = collections.defaultdict(list)
    for p in tracked_purls:
        all_by_name[strip_version(p)].append(p)

    buckets: dict[str, int] = collections.Counter()
    example: dict[str, str] = {}

    for o in orphans:
        eco = ecosystem_of(o)
        siblings = all_by_name.get(strip_version(o), [])
        reachable_siblings = [s for s in siblings if s in visited]
        has_incoming = len(incoming.get(o, [])) > 0
        # Multi-version co-existence with only some sibling reachable
        if reachable_siblings and eco == "npm":
            bucket = "dead-lockfile-entry"
        elif reachable_siblings and eco == "golang":
            bucket = "stale-go-sum-entry"
        elif not has_incoming and eco == "golang":
            # No sibling reachable AND no incoming → likely staging or generated
            if "staging" in o:
                bucket = "staging-repo-artifact"
            else:
                bucket = "unresolved-go-module"
        elif not has_incoming and eco == "npm":
            bucket = "hoisted-unused"
        elif "generic" in o or "file-tier" in (str(o).lower()):
            bucket = "file-tier-unattributed"
        else:
            bucket = "other-orphan"

        buckets[bucket] += 1
        example.setdefault(bucket, o)

    return {
        "orphans_total": len(orphans),
        "buckets": dict(buckets),
        "example_by_bucket": example,
    }


def per_tool_metrics(sbom: dict, wall_clock_seconds: float | None) -> dict:
    total = len(components_of(sbom))
    reachable, tracked = bfs_reachable(sbom)
    return {
        "total_components": total,
        "edges": edge_count(sbom),
        "bfs_reachable": reachable,
        "bfs_reachability_pct": (
            round(reachable / tracked * 100, 1) if tracked > 0 else 0.0
        ),
        "ecosystem_breakdown": ecosystem_breakdown(sbom),
        "empty_version_purls": empty_version_purl_count(sbom),
        "phantom_edges": phantom_edge_count(sbom),
        "wall_clock_seconds": wall_clock_seconds,
    }


def tool_comparison_delta(
    mikebom: dict, trivy: dict, syft: dict, ecosystem_prefix: str
) -> dict:
    def purl_set(sbom: dict) -> set[str]:
        return {
            purl_of(c)
            for c in components_of(sbom)
            if purl_of(c).startswith(ecosystem_prefix)
        }

    m, t, s = purl_set(mikebom), purl_set(trivy), purl_set(syft)
    mikebom_advantage = sorted(m - t - s)
    trivy_advantage = sorted(t - m - s)
    syft_advantage = sorted(s - m - t)
    all_three = m & t & s
    mikebom_trivy_only = sorted((m & t) - s)
    mikebom_syft_only = sorted((m & syft) - t) if False else sorted((m & s) - t)
    trivy_syft_only = sorted((t & s) - m)

    def cap(lst: list[str], n: int = 20) -> list[str]:
        if len(lst) <= n:
            return lst
        return lst[:n] + [f"... and {len(lst) - n} more"]

    return {
        "ecosystem_prefix": ecosystem_prefix,
        "mikebom_count": len(m),
        "trivy_count": len(t),
        "syft_count": len(s),
        "all_three_intersect": len(all_three),
        "mikebom_advantage_count": len(m - t - s),
        "mikebom_advantage_sample": cap(mikebom_advantage),
        "trivy_advantage_count": len(t - m - s),
        "trivy_advantage_sample": cap(trivy_advantage),
        "syft_advantage_count": len(s - m - t),
        "syft_advantage_sample": cap(syft_advantage),
        "mikebom_trivy_only_count": len(mikebom_trivy_only),
        "mikebom_syft_only_count": len(mikebom_syft_only),
        "trivy_syft_only_count": len(trivy_syft_only),
    }


def cross_ecosystem_interactions(sbom: dict) -> dict:
    """Detect Go↔npm edges. Emit sample edges and totals."""
    comp_ecosystem: dict[str, str] = {}
    for c in components_of(sbom):
        p = purl_of(c)
        eco = ecosystem_of(p)
        if eco in ("npm", "golang"):
            comp_ecosystem[p] = eco

    npm_to_golang: list[tuple[str, str]] = []
    golang_to_npm: list[tuple[str, str]] = []
    for d in sbom.get("dependencies") or []:
        ref = d.get("ref")
        if not isinstance(ref, str):
            continue
        src_eco = comp_ecosystem.get(ref)
        if src_eco not in ("npm", "golang"):
            continue
        for t in d.get("dependsOn") or []:
            if not isinstance(t, str):
                continue
            tgt_eco = comp_ecosystem.get(t)
            if tgt_eco not in ("npm", "golang"):
                continue
            if src_eco == "npm" and tgt_eco == "golang":
                npm_to_golang.append((ref, t))
            elif src_eco == "golang" and tgt_eco == "npm":
                golang_to_npm.append((ref, t))

    return {
        "npm_to_golang_count": len(npm_to_golang),
        "npm_to_golang_sample": [f"{s} → {t}" for s, t in npm_to_golang[:10]],
        "golang_to_npm_count": len(golang_to_npm),
        "golang_to_npm_sample": [f"{s} → {t}" for s, t in golang_to_npm[:10]],
    }


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--target-name", required=True, choices=["kubernetes", "argocd"])
    p.add_argument("--sboms-dir", required=True, type=pathlib.Path)
    p.add_argument("--commit-sha", required=True)
    args = p.parse_args()

    sboms_dir: pathlib.Path = args.sboms_dir
    if not sboms_dir.is_dir():
        print(f"error: sboms-dir does not exist: {sboms_dir}", file=sys.stderr)
        return 1

    mikebom_path = sboms_dir / "mikebom.cdx.json"
    trivy_path = sboms_dir / "trivy.cdx.json"
    syft_path = sboms_dir / "syft.cdx.json"

    mikebom = load_sbom(mikebom_path)
    trivy = load_sbom(trivy_path)
    syft = load_sbom(syft_path)

    output = {
        "schema": "milestone-165-analysis-v1",
        "target": args.target_name,
        "commit_sha": args.commit_sha,
        "per_tool_metrics": {
            "mikebom": per_tool_metrics(mikebom, None),
            "trivy": per_tool_metrics(trivy, None),
            "syft": per_tool_metrics(syft, None),
        },
        "mikebom_failure_modes": classify_mikebom_orphans(mikebom),
        "tool_comparison_delta": {
            "golang": tool_comparison_delta(mikebom, trivy, syft, "pkg:golang/"),
            "npm": tool_comparison_delta(mikebom, trivy, syft, "pkg:npm/"),
        },
        "cross_ecosystem_interactions": cross_ecosystem_interactions(mikebom),
        "invariant_checks": {
            # Milestone 163 SC-004 invariants
            "mikebom_empty_version_purls_is_zero":
                empty_version_purl_count(mikebom) == 0,
            "mikebom_phantom_edges_is_zero":
                phantom_edge_count(mikebom) == 0,
        },
    }

    json.dump(output, sys.stdout, indent=2, sort_keys=True)
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
