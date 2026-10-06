#!/usr/bin/env bash
# Bump the workspace version and everything that embeds it (#1133).
#
# Usage: scripts/release-bump.sh NEW_VERSION
#
# Used for both a release bump (RELEASING.md §2) and the post-release bump to
# the next development version (§2 step 10, automated by
# .github/workflows/post-release.yml). It:
#   1. sets [workspace.package] version in Cargo.toml;
#   2. updates only the workspace crates in Cargo.lock (`--workspace`): a
#      version bump must not upgrade dependencies. The same for
#      waybill-ebpf/Cargo.lock: the kernel crate is its own workspace, with
#      waybill-common as a path dependency, and was left at 0.1.0-alpha.3
#      for months because nothing updated it. It is run with the root
#      toolchain (`+<channel>` overrides waybill-ebpf/rust-toolchain.toml),
#      so a bump doesn't need the pinned eBPF nightly installed;
#   3. regenerates every golden (scripts/regen-goldens.sh);
#   4. checks the diff is the version and nothing else
#      (scripts/check-version-bump.py).
set -euo pipefail

NEW="${1:?usage: release-bump.sh NEW_VERSION}"
OLD=$(grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
if [ "$OLD" = "$NEW" ]; then
  echo "Cargo.toml is already $NEW" >&2
  exit 1
fi
echo ">>> bumping workspace $OLD -> $NEW"
perl -0pi -e "s/^version = \"\Q$OLD\E\"/version = \"$NEW\"/m" Cargo.toml
cargo update --workspace
channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
cargo "+$channel" update --workspace --manifest-path waybill-ebpf/Cargo.toml
./scripts/regen-goldens.sh
python3 scripts/check-version-bump.py "$OLD" "$NEW"
