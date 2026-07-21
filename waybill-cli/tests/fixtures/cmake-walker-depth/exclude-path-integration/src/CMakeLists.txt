# F6 remediation from /speckit-analyze — this file is under src/ which
# is NOT in milestone-156's walked top-level set (cmake/, Modules/,
# third_party/). Milestone-156 FR-017 forbids walking src/. Any
# find_package here MUST NOT be discovered.
find_package(NotEmitted 9.9.9)
