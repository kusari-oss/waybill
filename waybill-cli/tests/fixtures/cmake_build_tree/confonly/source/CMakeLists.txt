# Probe for spec 1653: one source tree, built in several configurations, to
# observe which build-tree files record each fetch outcome.
# DEPS is the absolute path of the generated local dependency sources
# (git repos and tarballs); nothing here touches the network.
cmake_minimum_required(VERSION 3.24)
project(probe LANGUAGES CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
include(FetchContent)
include(ExternalProject)

option(WITH_EXTRA "fetch extra" OFF)
set(BACKEND "A" CACHE STRING "backend selector")

# 1. FetchContent with FIND_PACKAGE_ARGS: taken unless hdronly is installed.
FetchContent_Declare(hdronly
  GIT_REPOSITORY ${DEPS}/hdronly.git
  GIT_TAG v1.2.3
  FIND_PACKAGE_ARGS 1.2 CONFIG)
# 2. FetchContent by URL with hash: always taken.
FetchContent_Declare(urldep
  URL ${DEPS}/urldep.tar.gz
  URL_HASH SHA256=${URLDEP_SHA256})
FetchContent_MakeAvailable(hdronly urldep)

# 2b. Mixed-case declared names (research R1's gap).
FetchContent_Declare(MixedCase URL ${DEPS}/mixedcase.tar.gz)
FetchContent_MakeAvailable(MixedCase)
ExternalProject_Add(ExtMixed
  URL ${DEPS}/extproj.tar.gz
  URL_HASH SHA256=${EXTPROJ_SHA256}
  CMAKE_ARGS -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>)

# 3. Declared, never made available.
FetchContent_Declare(neverused URL ${DEPS}/neverused.tar.gz)

# 4. Option-gated (default OFF).
if(WITH_EXTRA)
  FetchContent_Declare(extra URL ${DEPS}/extra.tar.gz)
  FetchContent_MakeAvailable(extra)
endif()

# 5. Cache-string gated (#1656).
if(BACKEND STREQUAL "B")
  FetchContent_Declare(bdep URL ${DEPS}/bdep.tar.gz)
  FetchContent_MakeAvailable(bdep)
endif()

# 6. ExternalProject by URL (download happens at build time).
ExternalProject_Add(extproj
  URL ${DEPS}/extproj.tar.gz
  URL_HASH SHA256=${EXTPROJ_SHA256}
  CMAKE_ARGS -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>)
# 7. ExternalProject by git.
ExternalProject_Add(extgit
  GIT_REPOSITORY ${DEPS}/extra.git
  GIT_TAG main
  CMAKE_ARGS -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>)

add_executable(app main.cpp)
if(TARGET hdronly::hdronly)
  target_link_libraries(app PRIVATE hdronly::hdronly)
else()
  target_link_libraries(app PRIVATE hdronly)
endif()
target_link_libraries(app PRIVATE urldep)
