#!/usr/bin/env python3
"""
Cross-tier SBOM correlation walker.

Demonstrates content-addressable correlation across waybill-emitted
source / build / image SBOMs without invoking waybill.

Reads identifiers from each SBOM's standards-native carriers:
- `metadata.component.externalReferences[]` for document-level identifiers
  (`repo:`, `git:`, `image:`, `subject:`, `attestation:`)
- `components[].hashes[]` for per-component content hashes

Then walks: image SBOM → build SBOM (via subject digest match) →
source SBOM (via `repo:` match). Pure string equality; no waybill
dependency.

Usage:
    python3 walk.py sboms/

Or specify the entry point:
    python3 walk.py sboms/ --image acme/widget-svc:v1
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any


def parse_external_ref(ref: dict[str, Any]) -> tuple[str, str] | None:
    """Decode a CDX externalReferences[] entry into a (scheme, value) pair.

    Waybill-emitted identifiers carry the scheme implicitly through `type`
    and the value through `url`. The `comment` field carries the
    `source_label` for human consumption.
    """
    ref_type = ref.get("type", "")
    url = ref.get("url", "")
    if ref_type == "vcs":
        # Both `repo:` and `git:` ride this type. Distinguish by `#`
        # fragment presence: `git:` values carry `<url>#<sha>`, `repo:`
        # values do not.
        scheme = "git" if "#" in url else "repo"
        return (scheme, url)
    if ref_type == "distribution":
        return ("image", url)
    if ref_type == "attestation":
        # Both `attestation:` IRIs and `subject:` digest identifiers ride
        # this type. Distinguish by value form: digest form is `<algo>:<hex>`.
        if url.startswith("sha256:") or url.startswith("sha512:"):
            return ("subject", url)
        return ("attestation", url)
    return None


def load_sbom(path: Path) -> dict[str, Any]:
    """Load a CDX 1.6 SBOM and surface its identifier set + component hashes."""
    with path.open("r", encoding="utf-8") as fp:
        sbom = json.load(fp)
    identifiers: list[tuple[str, str]] = []
    refs = (sbom.get("metadata", {}).get("component", {}).get("externalReferences", []))
    for ref in refs:
        parsed = parse_external_ref(ref)
        if parsed is not None:
            identifiers.append(parsed)
    component_hashes: list[tuple[str, str, str]] = []
    for comp in sbom.get("components", []):
        purl = comp.get("purl", comp.get("name", "<unknown>"))
        for h in comp.get("hashes", []):
            alg = h.get("alg", "").lower().replace("-", "")
            content = h.get("content", "").lower()
            if alg and content:
                component_hashes.append((purl, alg, content))
    tier = "unknown"
    for prop in sbom.get("metadata", {}).get("properties", []):
        if prop.get("name") == "waybill:sbom-tier":
            tier = prop.get("value", "unknown")
    return {
        "path": path,
        "tier": tier,
        "identifiers": identifiers,
        "component_hashes": component_hashes,
        "name": sbom.get("metadata", {}).get("component", {}).get("name", "<unnamed>"),
    }


def build_index(sboms: list[dict[str, Any]]) -> dict[tuple[str, str], list[dict[str, Any]]]:
    """Index SBOMs by every (scheme, value) identifier they carry.

    For `image:` identifiers, also index by the digest portion alone
    (after `@sha256:`) so it matches against `subject:sha256:<hex>`.
    """
    index: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for sbom in sboms:
        for scheme, value in sbom["identifiers"]:
            index[(scheme, value)].append(sbom)
            # Also index image: digests in subject: form for cross-tier
            # handshake via string match.
            if scheme == "image" and "@sha256:" in value:
                digest = value.split("@", 1)[1]  # sha256:<hex>
                index[("subject", digest)].append(sbom)
    return index


def correlate(image: dict[str, Any], index: dict[tuple[str, str], list[dict[str, Any]]]) -> dict[str, Any]:
    """Walk image → build → source by content-addressable string match."""
    result: dict[str, Any] = {"image": image, "build": None, "source": None, "match_kind": None}

    # Mode 1: image-level handshake.
    # The image's `image:` digest portion equals the build's `subject:`
    # value when the build emitted the image manifest as a subject.
    for scheme, value in image["identifiers"]:
        if scheme == "image" and "@sha256:" in value:
            digest = value.split("@", 1)[1]
            for candidate in index.get(("subject", digest), []):
                if candidate is image:
                    continue  # skip self
                if candidate["tier"] == "build":
                    result["build"] = candidate
                    result["match_kind"] = "image-digest handshake"
                    break

    # Mode 2: per-component walk (when mode 1 didn't find a match).
    # Each binary component's hash may match a build SBOM's `subject:`
    # value. The first matching build wins for the image-correlation
    # purpose; in real workflows operators may want all matches.
    if result["build"] is None:
        for purl, alg, hex_value in image["component_hashes"]:
            if alg != "sha256":
                continue
            digest_value = f"{alg}:{hex_value}"
            for candidate in index.get(("subject", digest_value), []):
                if candidate["tier"] == "build":
                    result["build"] = candidate
                    result["match_kind"] = f"per-component hash on `{purl}`"
                    break
            if result["build"] is not None:
                break

    # source via repo: match from build.
    if result["build"] is not None:
        for scheme, value in result["build"]["identifiers"]:
            if scheme == "repo":
                for candidate in index.get(("repo", value), []):
                    if candidate["tier"] == "source":
                        result["source"] = candidate
                        break
                break

    return result


def render(chain: dict[str, Any]) -> str:
    """Print the chain as a plain-text tree."""
    lines: list[str] = []
    image = chain["image"]
    build = chain["build"]
    source = chain["source"]
    lines.append(f"image  : {image['name']:<24}  ← {image['path'].name}")
    if build is not None:
        lines.append(f"  via  : {chain['match_kind']}")
        lines.append(f"build  : {build['name']:<24}  ← {build['path'].name}")
        if source is not None:
            for scheme, value in build["identifiers"]:
                if scheme == "repo":
                    lines.append(f"  via  : repo:{value}")
                    break
            lines.append(f"source : {source['name']:<24}  ← {source['path'].name}")
        else:
            lines.append("source : (no source SBOM correlated by `repo:` match)")
    else:
        lines.append("build  : (no build SBOM correlated)")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("sbom_dir", type=Path, help="directory containing CDX 1.6 SBOMs (*.cdx.json)")
    parser.add_argument("--image", help="filter to a specific image-tier SBOM by `metadata.component.name`")
    args = parser.parse_args()

    if not args.sbom_dir.is_dir():
        print(f"error: not a directory: {args.sbom_dir}", file=sys.stderr)
        return 2

    sbom_paths = sorted(args.sbom_dir.glob("*.cdx.json"))
    if not sbom_paths:
        print(f"error: no *.cdx.json files in {args.sbom_dir}", file=sys.stderr)
        return 2

    sboms = [load_sbom(p) for p in sbom_paths]
    index = build_index(sboms)

    image_sboms = [s for s in sboms if s["tier"] == "image"]
    if args.image:
        image_sboms = [s for s in image_sboms if s["name"] == args.image]
    if not image_sboms:
        print("error: no image-tier SBOMs found", file=sys.stderr)
        return 1

    for image in image_sboms:
        chain = correlate(image, index)
        print(render(chain))
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
