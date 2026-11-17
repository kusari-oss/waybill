#!/bin/bash
# Spec 1538 research R3: how CMake treats each fallback shape, measured with
# the installed cmake (configure only, no network: every fetch points at a
# local SOURCE_DIR). Usage: probe.sh <scratch-dir>. Prints one line per probe.
set -u
W=$1; rm -rf "$W"; mkdir -p "$W"
mk() { mkdir -p "$1"; printf 'cmake_minimum_required(VERSION 3.25)\nproject(%s NONE)\nset(%s_ORIGIN "%s" CACHE INTERNAL "")\n' "$2" "$2" "$1" > "$1/CMakeLists.txt"; }
cfg() { cmake -S "$1" -B "$1/_b" -Wno-dev "${@:2}" > "$1/_log" 2>&1; echo $?; }
mk "$W/src/a" fxa; mk "$W/src/b" fxb

# P1: root declares X (a), then add_subdirectory(sub), which declares X (b)
# with no guard and makes it available. Which source does X resolve to?
P=$W/p1; mkdir -p $P/sub
cat > $P/CMakeLists.txt <<C
cmake_minimum_required(VERSION 3.25)
project(p1 NONE)
include(FetchContent)
FetchContent_Declare(X SOURCE_DIR $W/src/a)
add_subdirectory(sub)
C
cat > $P/sub/CMakeLists.txt <<C
FetchContent_Declare(X SOURCE_DIR $W/src/b)
FetchContent_MakeAvailable(X)
message(STATUS "P1 X_SOURCE_DIR=\${x_SOURCE_DIR}")
C
r=$(cfg $P); echo "P1 first-declaration-wins across add_subdirectory: exit=$r $(grep -o 'P1 X_SOURCE_DIR=.*' $P/_log)"

# P2..P4: FIND_PACKAGE_ARGS and FETCHCONTENT_TRY_FIND_PACKAGE_MODE, with a
# findable package config for Y on CMAKE_PREFIX_PATH.
mkdir -p $W/prefix/lib/cmake/Y; echo 'set(Y_FOUND TRUE)' > $W/prefix/lib/cmake/Y/YConfig.cmake
probe_fpa() {  # <label> <mode or ""> <declare args>
  P=$W/$1; mkdir -p $P
  { echo 'cmake_minimum_required(VERSION 3.25)'; echo "project($1 NONE)"; echo 'include(FetchContent)'
    [ -n "$2" ] && echo "set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE $2)"
    echo "FetchContent_Declare(Y SOURCE_DIR $W/src/a $3)"
    echo 'FetchContent_MakeAvailable(Y)'
    echo 'message(STATUS "R found=${Y_FOUND} populated=${y_POPULATED} src=${y_SOURCE_DIR}")'; } > $P/CMakeLists.txt
  r=$(cfg $P -DCMAKE_PREFIX_PATH=$W/prefix); echo "$1 mode=${2:-default} args='$3': exit=$r $(grep -o 'R found=.*' $P/_log)"
}
probe_fpa P2a "" "FIND_PACKAGE_ARGS"
probe_fpa P2b "" ""
probe_fpa P3 ALWAYS ""
probe_fpa P4 NEVER "FIND_PACKAGE_ARGS"

# P5: a dependency provider. Where may it be set, and does it see
# ExternalProject_Add?
P=$W/p5; mkdir -p $P
cat > $P/provider.cmake <<C
macro(wf_provider method name)
  message(STATUS "P5 provider saw \${method} \${name}")
endmacro()
cmake_language(SET_DEPENDENCY_PROVIDER wf_provider SUPPORTED_METHODS FIND_PACKAGE FETCHCONTENT_MAKEAVAILABLE_SERIAL)
C
cat > $P/CMakeLists.txt <<C
cmake_minimum_required(VERSION 3.25)
project(p5 NONE)
include(FetchContent)
include(ExternalProject)
FetchContent_Declare(Z SOURCE_DIR $W/src/a)
FetchContent_MakeAvailable(Z)
ExternalProject_Add(E SOURCE_DIR $W/src/b CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
C
r=$(cfg $P -DCMAKE_PROJECT_TOP_LEVEL_INCLUDES=$P/provider.cmake); echo "P5 provider via CMAKE_PROJECT_TOP_LEVEL_INCLUDES: exit=$r $(grep -o 'P5 provider saw [A-Z_]* [A-Za-z]*' $P/_log | tr '\n' ';')"
P=$W/p5b; mkdir -p $P; cp $W/p5/provider.cmake $P/
cat > $P/CMakeLists.txt <<C
cmake_minimum_required(VERSION 3.25)
project(p5b NONE)
include(provider.cmake)
C
r=$(cfg $P); echo "P5b provider set from an ordinary CMakeLists include: exit=$r $(grep -m1 -oE 'CMake Error.*|can only be called.*' $P/_log)"

# P6: ExternalProject_Add of the same name twice.
P=$W/p6; mkdir -p $P/sub
cat > $P/CMakeLists.txt <<C
cmake_minimum_required(VERSION 3.25)
project(p6 NONE)
include(ExternalProject)
ExternalProject_Add(E SOURCE_DIR $W/src/a CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
add_subdirectory(sub)
C
cat > $P/sub/CMakeLists.txt <<C
ExternalProject_Add(E SOURCE_DIR $W/src/b CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
C
r=$(cfg $P); echo "P6 duplicate ExternalProject_Add: exit=$r $(grep -m1 -oE 'add_custom_target cannot create target "[^"]*"[^.]*' $P/_log)"
