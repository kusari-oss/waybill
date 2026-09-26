# Quickstart: declared licenses from manifests

**Feature**: `1010-manifest-declared-license` · **Date**: 2026-09-26

How to verify this feature by hand, and the recipe for adding the next ecosystem.

## Confirm the baseline first

Before changing anything, reproduce the absence. This repository is the most
convenient fixture, because both of its member crates inherit their license.

```sh
# Both crates declare inheritance rather than a literal value
grep -n "^license" Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml
#   Cargo.toml:9:license = "Apache-2.0"
#   waybill-cli/Cargo.toml:5:license.workspace = true
#   waybill-common/Cargo.toml:5:license.workspace = true
```

```sh
cargo build --release
./target/release/waybill --offline sbom scan --path . --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json=/tmp/self.cdx.json

# Baseline: zero licensed components
jq '[.components[]? | select((.licenses//[])|length>0)] | length' /tmp/self.cdx.json
```

Expect `0` before the change. After it, the two member crates carry `Apache-2.0`
inherited from `[workspace.package]` — which simultaneously exercises extraction
(FR-001), canonicalisation (FR-005) and inheritance (FR-011a).

## Verify each acceptance path

```sh
S=/tmp/self.cdx.json

# FR-002 — declared, not concluded
jq '[.components[]?.licenses[]?.license.acknowledgement] | group_by(.) |
    map({ack: .[0], n: length})' $S
# expect only "declared" on an --offline scan; "concluded" requires enrichment

# FR-012 — offline parity. Same command, same answer.
jq '[.components[]? | select((.licenses//[])|length>0)] | length' $S

# FR-016 — scan-root inherits only when exactly one main-module carries a license
jq '.metadata.component | {purl, licenses}' $S
# this repo has two main-modules, so expect NO scan-root license here
```

The last check matters: waybill's own workspace is a **negative** case for FR-016.
Two member crates means inheritance must *not* fire. A fixture with one crate is
needed for the positive case — do not read a blank scan-root here as a failure.

## Verify the preservation path (FR-004)

```sh
mkdir -p /tmp/lic && cd /tmp/lic && cat > Cargo.toml <<'TOML'
[package]
name = "waybill-fixture-oddlicense"
version = "0.1.0"
license = "AllRightsReserved"
TOML
cargo generate-lockfile 2>/dev/null || true
```

`AllRightsReserved` is not a valid SPDX expression. Expect it **preserved**, not
dropped:

```sh
waybill --offline sbom scan --path /tmp/lic --no-deep-hash \
  --format spdx-2.3-json --output spdx-2.3-json=/tmp/lic.spdx.json

jq '.packages[] | select(.name|test("oddlicense")) | .licenseDeclared' /tmp/lic.spdx.json
# expect a LicenseRef-<hash>, NOT "NOASSERTION"

jq '.hasExtractedLicensingInfos[]? | {licenseId, extractedText}' /tmp/lic.spdx.json
# expect extractedText == "AllRightsReserved"
```

Seeing `NOASSERTION` here means the reader dropped the value — the #957 behaviour
this feature supersedes.

> Fixture names use the `waybill-fixture-*` prefix deliberately. Real package
> coordinates in fixtures trip the repository's advisory scanning.

## Recipe: adding the next ecosystem

1. Find the production main-module site. Do **not** grep `licenses: Vec::new()` —
   122 of those exist and only 13 are the target. Instead find the function whose
   body emits the `main-module` role, and check it is not under `#[cfg(test)]`.
2. Confirm the manifest table is already parsed at that point. In every case
   examined it is; cargo has it three lines above.
3. Look up the row in [`contracts/license-extraction.md`](./contracts/license-extraction.md).
   **If the Evidence column says *to verify*, verify it against the named source
   before writing code** — Phase 0 found two of its own assumptions wrong this way.
4. Extract the raw value; combine several with the row's operator; call the shared
   ladder. Never construct `SpdxExpression` leniently first.
5. Add unit tests for all three outcomes: canonical, preserved, absent.
6. Regenerate goldens only once, at the end, across all ecosystems — and read the
   diff rather than accepting it.

## Two traps

**The emitter joins with `AND`.** `reduce_license_vec` concatenates multiple
licenses with `" AND "` unconditionally. If a reader pushes two values for an
ecosystem whose list means *choice*, the output asserts a consumer must satisfy
both. Combine in the reader; never rely on the default.

**#957 is superseded, not a template to copy.** Its Haskell implementation drops
an uncanonicalisable value. Copying that would spread the behaviour this feature
exists to replace. The parts worth copying are the ladder's first step and the
diagnostic; the `None` branch changes.
