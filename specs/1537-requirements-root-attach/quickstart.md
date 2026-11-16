# Quickstart: check declared requirements attachment

Build a release binary, then run #1566's reproduction (contract C-2):

```sh
cargo build --release -p waybill
W=$PWD/target/release/waybill
cd "$(mktemp -d)"
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
git init -q dep && git -C dep commit -q --allow-empty -m init
git init -q app
git -C app -c protocol.file.allow=always submodule -q add ../dep third_party/waybill-fixture-dep
printf 'waybill-fixture-a==1.0.0\nwaybill-fixture-b==2.0.0\n' > app/requirements.txt
git -C app add -A && git -C app commit -q -m add

$W --offline sbom scan --path app \
  --format cyclonedx-json --output cyclonedx-json=app.cdx.json \
  --format spdx-2.3-json  --output spdx-2.3-json=app.spdx23.json \
  --format spdx-3-json    --output spdx-3-json=app.spdx3.json
```

**Expected:** the root depends on the submodule and on both PyPI packages in every format, and the verdict is `complete`.

```sh
jq -r '.metadata.component["bom-ref"] as $r
  | .dependencies[] | select(.ref == $r) | .dependsOn[]' app.cdx.json | sort
jq -r '.metadata.properties[] | select(.name == "waybill:graph-completeness") | .value' app.cdx.json
```

**At `main` `283e4c19`** the first command lists only the submodule, while the second still prints `complete` (research R1).

The integration tests in `waybill-cli/tests/declared_requirements.rs` cover contract cases C-1 to C-9 and invariants I-1 to I-4:

```sh
cargo test -p waybill --test declared_requirements
```
