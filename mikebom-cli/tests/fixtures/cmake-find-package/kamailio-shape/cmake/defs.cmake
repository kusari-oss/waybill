# Milestone-155 SC-004 — depth-1 defs.cmake mirrors Kamailio's
# cmake/defs.cmake shape.

# Two additional find_package declarations (no version constraint).
find_package(Libev)
find_package(NETSNMP)

# One pkg_check_modules declaration.
pkg_check_modules(RADIUS REQUIRED IMPORTED_TARGET radcli)
