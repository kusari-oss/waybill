# Sigstore trust keys — vendoring recipe + rotation policy

**Scope**: How waybill sources, vendors, verifies, and rotates the
Sigstore CTFE (Certificate Transparency Front End) public keys that
back `waybill sbom scan --sign` (Sigstore keyless signing, US2b of
milestone 222).

**Audience**: Maintainers who need to (a) rotate keys when Sigstore
rotates its trust root, (b) unwind the `[patch.crates-io]` fork of
`sigstore-rs` once the upstream PR lands, or (c) audit the
provenance of what's in `waybill-cli/vendor/sigstore/`.

## Why vendored

`SigningContext::production()` in `sigstore-rs` is the sanctioned way
to obtain a signer against the public-good Sigstore infrastructure.
It requires the `sigstore-trust-root-rustls-tls` (or `-native-tls`)
feature, which transitively pulls the `tough` TUF client. `tough`
0.19 and 0.22 both have an **unconditional** `[dependencies.aws-lc-rs]`
entry — there is no `ring` alternative feature. `aws-lc-rs` is
C-native, which violates Constitution Principle I (Pure Rust, Zero C).

The alternative, `SigningContext::new(fulcio, rekor_config, ctfe_keyring)`,
accepts a hand-supplied `Keyring` of CTFE public keys — no TUF fetch,
no `tough`, no `aws-lc-rs`. Constructing that `Keyring` requires
DER-encoded SPKI bytes of the currently-trusted CTFE public keys,
which we vendor at build time.

Full audit trail in `specs/222-sigstore-keyless-signing/research.md §R1`.

## `[patch.crates-io]` fork provenance

`SigningContext::new()` is `pub fn` but its `Keyring` parameter type
lived in `pub(crate) mod keyring` in `sigstore-rs` 0.11.0 — a "phantom
public" API only reachable from inside the crate. We publish a
one-line patch that flips the module to `pub` and re-exports
`Keyring` at `sigstore::crypto::Keyring`.

| Field | Value |
|-------|-------|
| Fork repo | `github.com/kusari-sandbox/sigstore-rs` |
| Fork branch | `waybill/expose-keyring` |
| Fork tag | `v0.11.0-waybill-1` |
| Base tag | `v0.11.0` (upstream `sigstore/sigstore-rs`) |
| Upstream PR | `sigstore/sigstore-rs#610` |
| Diff | 3 lines added, 1 removed — see `src/crypto/mod.rs` on the branch |

Wired via `[patch.crates-io]` in the workspace `Cargo.toml`:

```toml
[patch.crates-io]
sigstore = { git = "https://github.com/kusari-sandbox/sigstore-rs.git", tag = "v0.11.0-waybill-1" }
```

### Un-patch conditions

Remove the `[patch.crates-io]` entry as soon as **both** of:

1. Upstream PR `sigstore/sigstore-rs#610` is merged.
2. A `sigstore` crate release containing the merge is published to
   crates.io (bump the version pin in `waybill-cli/Cargo.toml:161`).

If upstream declines the PR long-term, the fork is our permanent
substrate; rebase on each new `sigstore-rs` release per the sync
policy below.

### Fork sync policy

When upstream ships `sigstore-rs vX.Y.Z`:

1. `git fetch upstream` (upstream = `sigstore/sigstore-rs`).
2. `git checkout waybill/expose-keyring && git rebase vX.Y.Z`.
3. Resolve conflicts (should only be `src/crypto/mod.rs` — the
   `pub(crate) mod keyring` line).
4. `git tag vX.Y.Z-waybill-N` (increment `N` if we ship multiple
   fork rebases for the same upstream release).
5. Push the branch + tag.
6. Bump `Cargo.toml` `[patch.crates-io]` tag pin + `waybill-cli/Cargo.toml`
   `sigstore = "X.Y.Z"` version.
7. Re-run T001 audit (`cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|aws-lc-sys|native-tls|mbedtls-sys|tough'` must return zero hits).
8. Re-run `docs/sigstore-trust-keys.md` recipe below (§Rotation) if
   Sigstore has also rotated CTFE keys since last vendoring.

## Vendored files

`waybill-cli/vendor/sigstore/`:

| File | Size | SHA-256 | LogID (base64) | validFor.start |
|------|------|---------|----------------|----------------|
| `ctfe_prod.der` | 91 B | `dd3d306a…f29ee8e` | `3T0wasbHETJjGR4cmWc3AqJKXrjePK3/h4pygC8p7o4=` | 2022-10-20 |
| `ctfe_stage_20220701.der` | 91 B | `2b30bcdc…ac867a` | `KzC83GiIyeLh2CYpXnQfSDkxlgLynDPLXkNA/rKshno=` | 2022-07-01 |
| `ctfe_stage_20260114.der` | 91 B | `3e607153…a0a6b6` | `PmBxU3RuGJLgkLI2sUl2Jy9ntE1vks5vdxFKtyKgprY=` | 2026-01-14 |
| `ctfe_stage_20260612.der` | 91 B | `1638fb66…5ebc87` | `Fjj7Zk5I004nmb83WUyHasaCMoUO4fuTnFybtSxevIc=` | 2026-06-12 |

Each file is a raw DER-encoded SPKI (SubjectPublicKeyInfo) blob for a
NIST P-256 ECDSA public key (`prime256v1`, key size 256). Sigstage
carries 3 currently-active CTLogs (Rekor may write SCTs to any of
them); production carries 1 (the 2022-10-20 log).

## Vendoring recipe (used at T002)

**Environment at vendoring time (2026-07-30)**:

| Tool | Version | Provenance |
|------|---------|------------|
| cosign | `v3.1.2` | `GitCommit 193d2153431f8bb0d945a4c1ee721872f73add67`, `BuildDate 2026-07-17T14:32:20Z` (Homebrew `/opt/homebrew/bin/cosign`) |
| Sigstore prod trust root | `root.json` version 15, expires 2026-11-20 | SHA-256 `73747011d0857ada15479a16c4cae0f3ed03aac698b523b97e1de314ac9d9ca8` |
| Sigstore stage trust root | `root.json` version 14, expires 2026-10-16 | SHA-256 `353d172f05f9e73b4a647b1a464f16726d929cbfd030cc42bffb86cd742fb61f`; bootstrap version-12 root fetched via `https://tuf-repo-cdn.sigstage.dev/12.root.json` |

**Recipe**:

```bash
# Fresh state
rm -rf ~/.sigstore

# 1. Fetch + TUF-verify production trust root
cosign initialize

# 2. Extract active production CTFE key (validFor.end == null)
mkdir -p waybill-cli/vendor/sigstore
jq -r '.ctlogs | map(select(.publicKey.validFor.end == null)) | .[0].publicKey.rawBytes' \
    ~/.sigstore/root/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json \
    | base64 -d > waybill-cli/vendor/sigstore/ctfe_prod.der

# 3. Fetch sigstage bootstrap root (version 12 at time of vendoring;
#    increment if a newer version is published)
curl -sS https://tuf-repo-cdn.sigstage.dev/12.root.json > /tmp/sigstage-root.json

# 4. Fetch + TUF-verify sigstage trust root
cosign initialize --mirror https://tuf-repo-cdn.sigstage.dev --root /tmp/sigstage-root.json

# 5. Extract ALL currently-active sigstage CTFE keys (validFor.end == null).
#    Sigstage runs multiple concurrent CTLogs; we include each so Rekor
#    can pick any without breaking sign-flow SCT verification.
jq -r '.ctlogs | to_entries | map(select(.value.publicKey.validFor.end == null))
       | .[] | "\(.value.publicKey.validFor.start)\t\(.value.publicKey.rawBytes)"' \
    ~/.sigstore/root/tuf-repo-cdn.sigstage.dev/targets/trusted_root.json \
    | while IFS=$'\t' read -r start rawb64; do
        suffix=$(echo "$start" | sed 's/[-:T]//g' | cut -c1-8)
        echo "$rawb64" | base64 -d > waybill-cli/vendor/sigstore/ctfe_stage_${suffix}.der
      done

# 6. Verify each file is a valid P-256 SPKI
for f in waybill-cli/vendor/sigstore/*.der; do
  openssl pkey -pubin -inform DER -in "$f" -noout -text \
    | grep -E 'Public-Key|NIST|ASN1' | head -3
done
```

If steps 2 or 5 produce files whose SHAs differ from the table above,
Sigstore has rotated a key. Update the SHA table + the `include_bytes!`
constants list in `waybill-cli/src/attestation/sigstore_trust_root.rs`
accordingly, then update the `validFor.start` column with the new
values from the trust root.

## Rotation policy

**Expected cadence**: Sigstore rotates CTFE keys roughly once per
year; sigstage rotates more often (typically 2–4x/year, sometimes
more with cluster churn). We do **not** need to rotate for every
sigstage rotation as long as at least one currently-active sigstage
key is still vendored — Rekor will use it. Full re-vendoring is only
required when:

- Production adds a new CTFE key (verify against upstream Sigstore
  release notes or `cosign initialize` output diff).
- The active sigstage CTFE set changes such that Rekor is writing
  SCTs to a key we don't vendor, causing the integration test
  (`lint-and-test-keyless-sbom`) to fail.

**Detection signal**: CI job `lint-and-test-keyless-sbom` failure
with an SCT-verification error class — sigstore-rs error message
like `"no known key for log ID <base64>"`. When this fires, run the
vendoring recipe above; commit the updated DER files + updated
`SIGSTORE_STAGE_CTFE_KEYS_DER` array + updated `docs/sigstore-trust-keys.md`
SHA table in a single PR titled `chore: rotate Sigstore CTFE keys`.

**Cost estimate**: ~30 min of human effort per rotation cycle.

## Adding a fresh sigstage key mid-cycle (fast-path)

If CI trips on a new sigstage log without a full re-vendoring:

```bash
NEW_LOGID='<from-CI-error>'    # base64 log ID surfaced in the error
NEW_KEY_B64=$(jq -r --arg id "$NEW_LOGID" \
    '.ctlogs[] | select(.logId.keyId == $id) | .publicKey.rawBytes' \
    ~/.sigstore/root/tuf-repo-cdn.sigstage.dev/targets/trusted_root.json)
NEW_START=$(jq -r --arg id "$NEW_LOGID" \
    '.ctlogs[] | select(.logId.keyId == $id) | .publicKey.validFor.start' \
    ~/.sigstore/root/tuf-repo-cdn.sigstage.dev/targets/trusted_root.json)
SUFFIX=$(echo "$NEW_START" | sed 's/[-:T]//g' | cut -c1-8)
echo "$NEW_KEY_B64" | base64 -d > waybill-cli/vendor/sigstore/ctfe_stage_${SUFFIX}.der
```

Then add the new `include_bytes!` line to `SIGSTORE_STAGE_CTFE_KEYS_DER`
in `waybill-cli/src/attestation/sigstore_trust_root.rs` and re-run CI.

## Cross-references

- `waybill-cli/vendor/sigstore/*.der` — the vendored files themselves
- `waybill-cli/src/attestation/sigstore_trust_root.rs` — consuming module
- `docs/cisa-2026-coverage.md` — row 2 (SBOM Author Signature) which
  US2b unblocks
- `specs/222-sigstore-keyless-signing/research.md §R1` — full audit
  trail for why the vendored-CTFE path was chosen
- Upstream PR: `sigstore/sigstore-rs#610` — the fork's un-patch
  target
