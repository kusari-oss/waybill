# Milestone-155 SC-004 synthetic Kamailio-shape testbed.
#
# Depth-1 walker will discover:
#   - <root>/CMakeLists.txt         (this file)
#   - <root>/cmake/defs.cmake
#
# It will NOT discover <root>/cmake/modules/FindLibev.cmake (depth-2).
# That file is included here for shape realism only; its
# find_package_handle_standard_args call would demonstrate FR-009
# negation ONCE the walker is extended in a future milestone.

cmake_minimum_required(VERSION 3.10)
project(kamailio-shape C)

# 3 top-level find_package calls (one runtime + two build-tool).
# Q2 clarification: build-tool packages (Threads) are emitted uniformly
# — no denylist in milestone 155.
find_package(OpenSSL 1.1.0 REQUIRED)
find_package(Threads REQUIRED)
find_package(ZLIB REQUIRED)

# Pull in depth-1 defs — 2 more find_package + 1 pkg_check_modules.
include(cmake/defs.cmake)

add_subdirectory(src)
