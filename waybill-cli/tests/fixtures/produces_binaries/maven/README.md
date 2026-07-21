# maven fixtures — `waybill:produces-binaries` (milestone 116 PR-B)

| Sub-fixture | POM shape | Expected (extensionless per spec Q2) |
|---|---|---|
| `shade-plugin/` | `<maven-shade-plugin>` with `<finalName>fixture-baz</finalName>` | `["fixture-baz"]` |
| `jar-plugin/` | `<maven-jar-plugin>` with `<finalName>fixture-baz</finalName>` | `["fixture-baz"]` |
| `library-only/` | No shade-/jar-plugin `<finalName>` | property OMITTED |
