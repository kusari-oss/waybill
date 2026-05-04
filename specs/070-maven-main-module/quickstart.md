# Quickstart: Verify maven main-module emission

Four recipes covering single-module, multi-module reactor, parent
inheritance, and property substitution.

## Prerequisites

```sh
cargo +stable build -p mikebom
```

## Recipe A — Single-module Maven project

```sh
mkdir -p /tmp/mvn-070
cat > /tmp/mvn-070/pom.xml <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>my-app</artifactId>
  <version>1.2.3</version>
  <packaging>jar</packaging>
</project>
EOF

target/debug/mikebom sbom scan --path /tmp/mvn-070 --format cyclonedx-json --output /tmp/mvn.cdx.json --no-deep-hash

jq '.metadata.component | {bom_ref: ."bom-ref", type, name, version, purl}' /tmp/mvn.cdx.json
```

**Expect**:

```json
{
  "bom_ref": "pkg:maven/com.example/my-app@1.2.3",
  "type": "application",
  "name": "my-app",
  "version": "1.2.3",
  "purl": "pkg:maven/com.example/my-app@1.2.3"
}
```

## Recipe B — Multi-module reactor

```sh
mkdir -p /tmp/mvn-reactor/{module-a,module-b}
cat > /tmp/mvn-reactor/pom.xml <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>parent</artifactId>
  <version>1.0.0</version>
  <packaging>pom</packaging>
  <modules>
    <module>module-a</module>
    <module>module-b</module>
  </modules>
</project>
EOF
cat > /tmp/mvn-reactor/module-a/pom.xml <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>module-a</artifactId>
</project>
EOF
cat > /tmp/mvn-reactor/module-b/pom.xml <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0.0</version>
  </parent>
  <artifactId>module-b</artifactId>
</project>
EOF

target/debug/mikebom sbom scan --path /tmp/mvn-reactor --format spdx-2.3-json --output /tmp/reactor.spdx.json --no-deep-hash

jq '[.packages[] | select(.primaryPackagePurpose == "APPLICATION") | { name, purl: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) }] | sort_by(.name)' /tmp/reactor.spdx.json
```

**Expect**:

```json
[
  { "name": "module-a", "purl": "pkg:maven/com.example/module-a@1.0.0" },
  { "name": "module-b", "purl": "pkg:maven/com.example/module-b@1.0.0" },
  { "name": "parent",   "purl": "pkg:maven/com.example/parent@1.0.0" }
]
```

`module-a` and `module-b` inherit `<groupId>` and `<version>` from the parent's `<parent>` block per Maven specification.

`documentDescribes` should contain all 3 SPDXIDs:

```sh
jq '.documentDescribes | length' /tmp/reactor.spdx.json
# → 3
```

## Recipe C — Property substitution (`${revision}` flatten plugin)

```sh
mkdir -p /tmp/mvn-rev
cat > /tmp/mvn-rev/pom.xml <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>flat-app</artifactId>
  <version>${revision}</version>
  <properties>
    <revision>2.0.0</revision>
  </properties>
</project>
EOF

target/debug/mikebom sbom scan --path /tmp/mvn-rev --format cyclonedx-json --output /tmp/rev.cdx.json --no-deep-hash
jq '.metadata.component.purl' /tmp/rev.cdx.json
```

**Expect**: `"pkg:maven/com.example/flat-app@2.0.0"` (`${revision}` resolved from `<properties>` per FR-012).

## Recipe D — Unresolved property (verbatim + warn)

```sh
mkdir -p /tmp/mvn-unresolved
cat > /tmp/mvn-unresolved/pom.xml <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>broken-app</artifactId>
  <version>${some.undefined.prop}</version>
</project>
EOF
RUST_LOG=mikebom=warn target/debug/mikebom sbom scan --path /tmp/mvn-unresolved --format cyclonedx-json --output /tmp/unresolved.cdx.json --no-deep-hash 2>&1 | grep -i "unresolved\|undefined"
jq '.metadata.component.purl' /tmp/unresolved.cdx.json
```

**Expect**: a `WARN` line about the unresolved property, plus the PURL emitted with the verbatim placeholder string (URL-encoded):
`"pkg:maven/com.example/broken-app@%24%7Bsome.undefined.prop%7D"` or similar (the `${...}` characters get percent-encoded by `build_maven_purl`'s segment encoder; this is the documented signal that resolution failed).

## When to run

- **Recipe A** during US1 / SC-001 verification
- **Recipe B** for FR-002 / SC-002 (multi-module reactor)
- **Recipe C** for FR-012 (property substitution success path)
- **Recipe D** for FR-001 step 4 + Edge Cases (unresolved property)

All four recipes should be exercised as integration tests in `tests/scan_maven.rs`.
