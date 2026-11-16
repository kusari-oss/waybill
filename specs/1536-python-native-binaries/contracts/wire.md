# Wire contract: owned binaries

The fixture: `waybill-fixture-pkg` 1.2.3, whose `RECORD` lists `waybill_fixture_pkg/.dylibs/libz.1.3.2.dylib`. The library carries an embedded zlib 1.3.2 version string and links `libSystem.B.dylib`. The shape is the same in both layouts (FR-006). `<sha>` is the library's SHA-256.

## CycloneDX 1.6

```json
{
  "components": [
    { "bom-ref": "pkg:pypi/waybill-fixture-pkg@1.2.3", "purl": "pkg:pypi/waybill-fixture-pkg@1.2.3",
      "components": [
        { "bom-ref": "pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>#pkg:pypi/waybill-fixture-pkg@1.2.3",
          "purl": "pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>",
          "evidence": { "occurrences": [ { "location": "waybill_fixture_pkg/.dylibs/libz.1.3.2.dylib" } ] },
          "components": [
            { "bom-ref": "pkg:generic/zlib@1.3.2#pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>#pkg:pypi/waybill-fixture-pkg@1.2.3",
              "purl": "pkg:generic/zlib@1.3.2" } ] } ] },
    { "bom-ref": "pkg:generic/%2Fusr%2Flib%2FlibSystem.B.dylib", "purl": "pkg:generic/%2Fusr%2Flib%2FlibSystem.B.dylib" }
  ],
  "dependencies": [
    { "ref": "pkg:pypi/waybill-fixture-pkg@1.2.3",
      "dependsOn": ["pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>#pkg:pypi/waybill-fixture-pkg@1.2.3"] },
    { "ref": "pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>#pkg:pypi/waybill-fixture-pkg@1.2.3",
      "dependsOn": ["pkg:generic/%2Fusr%2Flib%2FlibSystem.B.dylib",
                    "pkg:generic/zlib@1.3.2#pkg:generic/libz.1.3.2.dylib?file-sha256=<sha>#pkg:pypi/waybill-fixture-pkg@1.2.3"] }
  ],
  "metadata": { "properties": [ { "name": "waybill:python-owned-binaries", "value": "{\"analysed\":1,\"identified\":1}" } ] }
}
```

**Contract:**
- **C-1:** an owned binary appears exactly once, nested under its owner. When the owner is the document root (an unpacked wheel), that means at the top of `components[]`, which holds the root's contents (research R5).
- **C-2:** no `file`-tier component exists for the binary's path.
- **C-3:** every `dependencies` ref and `dependsOn` value is a `bom-ref` in the document.
- **C-4:** the linkage component stays top-level. It is a library outside the package.

## SPDX 2.3

- Packages: the owner, the binary, `zlib@1.3.2` and `libSystem.B.dylib`. One package per PURL: a library two packages vendor is one package that both `CONTAINS` (data-model V4).
- Relationships:
  - `owner CONTAINS binary`;
  - `binary CONTAINS zlib`;
  - `owner DEPENDS_ON binary`;
  - `binary DEPENDS_ON zlib`;
  - `binary DEPENDS_ON libSystem`.
- C230 is a document annotation, in the C228 envelope shape.

## SPDX 3.0.1

- The same elements, with `contains` and `dependsOn` relationships with the same endpoints.
- C230 is a document annotation.
- It must pass `spdx3-validate` 0.0.5.

## Unchanged

- A binary claimed by dpkg, rpm, apk, alpm, opkg or `.ipk` keeps today's output, whether or not a `RECORD` lists it.
- A tree without a `.dist-info` keeps today's flat output.
- C230 is absent when no owned binary was analysed.
