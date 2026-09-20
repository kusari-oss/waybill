# Pre-change baseline (T001, T002)

Binary: waybill 0.9.0 release, preserved at `/private/tmp/claude-501/-Users-mlieberman-Projects-mikebom/c5ea05ae-243f-4c56-b256-5b577e3c2ae1/scratchpad/m922b/waybill-prechange` for the T030 teeth-check.

## Per-fixture split output, BEFORE any edit

This is the FR-004 / SC-005 evidence. Byte-identity for non-colliding
repositories is unprovable after the fact; this table is what T016 spends.

### `pants_coursier_jvm_multi_resolve`

```
default.generic.cdx.json           pkg:maven/dev.waybill.fixture/runtime-a@1.0.0, pkg:maven/dev.waybill.fixture/runtime-b@1.0.0
junit.generic.cdx.json             pkg:maven/dev.waybill.fixture/testing-junit-a@1.0.0, pkg:maven/dev.waybill.fixture/testing-junit-b@1.0.0
scalatest.generic.cdx.json         pkg:maven/dev.waybill.fixture/testing-scala-a@1.0.0, pkg:maven/dev.waybill.fixture/testing-scala-b@1.0.0
manifest: subproject_id=default.generic        root_purl=pkg:generic/default
manifest: subproject_id=junit.generic          root_purl=pkg:generic/junit
manifest: subproject_id=scalatest.generic      root_purl=pkg:generic/scalatest
```

### `pants_discovered_resolves`

```
default.generic.cdx.json           pkg:pypi/waybill-fixture-alpha@1.0.0, pkg:pypi/waybill-fixture-beta@1.0.0
lint.generic.cdx.json              pkg:pypi/waybill-fixture-gamma@1.0.0
manifest: subproject_id=default.generic        root_purl=pkg:generic/default
manifest: subproject_id=lint.generic           root_purl=pkg:generic/lint
```

### `pants_namespace_collision`

```
default.generic.cdx.json           pkg:maven/dev.waybill.fixture/waybill-fixture-jvmside@1.0.0, pkg:pypi/waybill-fixture-pyside@1.0.0
lint.generic.cdx.json              pkg:pypi/waybill-fixture-lintside@1.0.0
manifest: subproject_id=default.generic        root_purl=pkg:generic/default
manifest: subproject_id=lint.generic           root_purl=pkg:generic/lint
```

### `pants_pex`

```
(no split output — the not-partitionable fallback fired)
```

### `pants_resolve_edges`

```
app.generic.cdx.json               pkg:pypi/waybill-fixture-common@1.0.0, pkg:pypi/waybill-fixture-consumer-a@1.0.0, pkg:pypi/waybill-fixture-shared@1.0.0
tools.generic.cdx.json             pkg:pypi/waybill-fixture-common@1.0.0, pkg:pypi/waybill-fixture-consumer-b@1.0.0, pkg:pypi/waybill-fixture-shared@2.0.0
manifest: subproject_id=app.generic            root_purl=pkg:generic/app
manifest: subproject_id=tools.generic          root_purl=pkg:generic/tools
```

## The defect (T002)

`pants_namespace_collision` emits ONE `default.*` document holding both a
Maven jar and a PyPI wheel — two unrelated resolves inside a document that
claims to be one resolve. `pants_pex` produces no split at all, which is the
legitimate single-resolve fallback and must survive (FR-009).
