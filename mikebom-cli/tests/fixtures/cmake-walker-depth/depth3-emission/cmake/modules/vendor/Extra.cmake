# Depth-3 file under cmake/modules/vendor/. Milestone 156 discovers
# this via safe_walk recursive descent under cmake/.
find_package(Foo 2.5)
# F5 remediation from /speckit-analyze — also test pkg_check_modules
# at depth-3 to verify FR-007 emission-shape preservation.
pkg_check_modules(BAR REQUIRED bar)
