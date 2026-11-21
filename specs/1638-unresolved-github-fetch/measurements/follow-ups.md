# T020: follow-up issues

- **#1646**: a non-GitHub `ExternalProject_Add` git fetch has no repository identity, even when its tag resolves (research R3; pinned by U-16).
- **#1647**: CMake `waybill:download-url` records `GIT_REPOSITORY` and `URL` verbatim, credentials included (spec Edge Cases). `sanitize_userinfo` has no CMake call site.
- **Observed, not filed:** two fetches with the same declared name in one file, of different repositories, give one component, on `main` too, including when both tags resolve. Found while building U-xhost: a `FetchContent_Declare(waybill-fixture-target …)` and an `ExternalProject_Add(waybill-fixture-target …)` of two repositories. CMake itself rejects a duplicate target name, so this case may never occur in a buildable tree. Recorded here, not traced to its cause.
