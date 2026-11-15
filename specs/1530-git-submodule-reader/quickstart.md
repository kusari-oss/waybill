# Quickstart: verifying git submodules

All of this runs on local `file://` repositories. Nothing is cloned from a remote.

## 1. The #1515 reproduction (SC-001)

```sh
S=$(mktemp -d); cd "$S"
git init -q dep && git -C dep commit -q --allow-empty -m init
git init -q app && cd app
git -c protocol.file.allow=always submodule -q add ../dep third_party/waybill-fixture-dep
git commit -q -m add && cd ..
git -c protocol.file.allow=always clone -q app fresh      # submodule left uninitialised
for d in app fresh; do
  waybill sbom scan --path "$d" --format cyclonedx-json --output "$d.cdx.json"
  jq -r '[.components[] | select(any(.properties[]?; .value=="git-submodule")) | .purl] | .[]' "$d.cdx.json"
  jq -r '.metadata.properties[]? | select(.name=="waybill:git-submodule-coverage") | .value' "$d.cdx.json"
done
```

**Expect.** Each scan gives one `pkg:generic/dep@<commit>?vcs_url=git+…` component, and `<commit>` equals `git -C dep rev-parse HEAD`. The coverage record reads `"pinned":1` with every skip at 0. On v0.16.0, both scans give nothing.

## 2. More declarations than gitlinks (SC-002)

Add a second `[submodule]` section to `app/.gitmodules` for a path that has no gitlink, commit, and rescan.

**Expect.** Still one component, and `"no-gitlink":1`.

## 3. No repository data (SC-009)

```sh
rm -rf fresh/.git && waybill sbom scan --path fresh --format cyclonedx-json --output bare.cdx.json
```

**Expect.** No submodule components, and `"pinned":0` with `"no-repository-data":1`.

## 4. A hostile repository config does not run (research R1)

```sh
printf '#!/bin/sh\ntouch %s/PWNED\n' "$S" > "$S/payload" && chmod +x "$S/payload"
git -C app config core.fsmonitor "$S/payload"
waybill sbom scan --path app --format cyclonedx-json --output hostile.cdx.json
test ! -e "$S/PWNED" && echo "payload did not run"
```

## 5. The measurements

```sh
specs/1530-git-submodule-reader/measurements/probe/git-safety.sh "$(mktemp -d)"
```

**Expect.** This re-runs research R1–R3. Compare its output with `measurements/git-safety.txt`.
