# maven-multi-module-reactor fixture (milestone 070)

3-module Maven reactor exercising:

- Parent POM (`com.example:parent:1.0.0`, packaging=pom) declaring
  `<modules>`: `module-a`, `module-b`.
- `module-a/pom.xml` inherits `<groupId>` and `<version>` from the
  `<parent>` block (FR-001 step 2 — POM inheritance).
- `module-b/pom.xml` inherits `<groupId>` and uses
  `<version>${project.version}</version>` for property substitution
  coverage (FR-012 + US1 AS#4).

All three resolve to:
- `pkg:maven/com.example/parent@1.0.0`
- `pkg:maven/com.example/module-a@1.0.0`
- `pkg:maven/com.example/module-b@1.0.0`

`documentDescribes` lists all three SPDXIDs (alphabetically sorted)
via the milestone-064-#127 multi-DESCRIBES infrastructure.

Used by integration tests in `tests/scan_maven.rs`.
