// m235 US3 fixture — multi-subproject cold-clone.
// No wrapper, no cache, no lockfile. US3 static parser produces
// direct-only components for both subprojects.
rootProject.name = "waybill-fixture-cold-clone"
include("app")
include(":core")
