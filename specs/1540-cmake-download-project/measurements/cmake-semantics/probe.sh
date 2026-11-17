#!/bin/bash
# Spec 1540 research: what a download_project() call fetches, measured with
# the installed cmake on a synthetic module with DownloadProject's mechanism
# (keyword lists in variables, cmake_parse_arguments(DL_ARGS ...), a
# configure_file()d template holding ExternalProject_Add, a nested configure
# and build). No network: tarballs are served from localhost, each holding
# one file `origin` that names it. Usage: probe.sh <scratch-dir>.
set -u
W=$1; rm -rf "$W"; mkdir -p "$W/tars" "$W/mod"
for t in a b; do mkdir -p "$W/tars/$t"; echo "$t" > "$W/tars/$t/origin"; tar -C "$W/tars/$t" -czf "$W/tars/$t-1.0.tar.gz" origin; done
PORT=${PORT:-18540}
python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$W/tars" > "$W/http.log" 2>&1 &
HTTP=$!; trap 'kill $HTTP 2>/dev/null' EXIT; sleep 1
A=http://127.0.0.1:$PORT/a-1.0.tar.gz; B=http://127.0.0.1:$PORT/b-1.0.tar.gz

# module(<dir> <extra one-value keyword or "">)
module() {
  mkdir -p "$1"
  cat > "$1/DownloadProject.cmake" <<C
set(_DownloadProjectDir "\${CMAKE_CURRENT_LIST_DIR}")
function(download_project)
  set(options QUIET)
  set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT $2)
  set(multiValueArgs "")
  cmake_parse_arguments(DL_ARGS "\${options}" "\${oneValueArgs}" "\${multiValueArgs}" \${ARGN})
  if(NOT DL_ARGS_PREFIX)
    set(DL_ARGS_PREFIX "\${CMAKE_BINARY_DIR}")
  endif()
  if(NOT DL_ARGS_DOWNLOAD_DIR)
    set(DL_ARGS_DOWNLOAD_DIR "\${DL_ARGS_PREFIX}/\${DL_ARGS_PROJ}-download")
  endif()
  if(NOT DL_ARGS_SOURCE_DIR)
    set(DL_ARGS_SOURCE_DIR "\${DL_ARGS_PREFIX}/\${DL_ARGS_PROJ}-src")
  endif()
  message(STATUS "PROBE proj=\${DL_ARGS_PROJ} unparsed=[\${DL_ARGS_UNPARSED_ARGUMENTS}] src=\${DL_ARGS_SOURCE_DIR}")
  configure_file("\${_DownloadProjectDir}/DownloadProject.CMakeLists.cmake.in" "\${DL_ARGS_DOWNLOAD_DIR}/CMakeLists.txt")
  execute_process(COMMAND \${CMAKE_COMMAND} . RESULT_VARIABLE r OUTPUT_QUIET ERROR_QUIET WORKING_DIRECTORY "\${DL_ARGS_DOWNLOAD_DIR}")
  execute_process(COMMAND \${CMAKE_COMMAND} --build . RESULT_VARIABLE r2 OUTPUT_QUIET ERROR_QUIET WORKING_DIRECTORY "\${DL_ARGS_DOWNLOAD_DIR}")
  message(STATUS "PROBE configure=\${r} build=\${r2}")
endfunction()
C
  cat > "$1/DownloadProject.CMakeLists.cmake.in" <<'C'
cmake_minimum_required(VERSION 3.18)
project(${DL_ARGS_PROJ}-download NONE)
include(ExternalProject)
if(${DL_ARGS_BUILD_PROJECT})
  ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS}
    SOURCE_DIR "${DL_ARGS_SOURCE_DIR}" BUILD_IN_SOURCE TRUE TEST_COMMAND ""
    CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
else()
  ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS}
    SOURCE_DIR "${DL_ARGS_SOURCE_DIR}" BUILD_IN_SOURCE TRUE TEST_COMMAND ""
    UPDATE_COMMAND "" CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
endif()
C
}
module "$W/mod/std" ""
module "$W/mod/extra" "EXTRA_KW"

# run <label> <module> <call...>: configure a project that includes the module
# and makes one call; print the PROBE lines and which tarball landed.
run() {
  local L=$1 P=$W/$1 M=$2; shift 2
  mkdir -p "$P"
  printf 'cmake_minimum_required(VERSION 3.18)\nproject(%s NONE)\ninclude(%s/DownloadProject.cmake)\n%s\n' "$L" "$M" "$*" > "$P/CMakeLists.txt"
  cmake -S "$P" -B "$P/_b" -Wno-dev > "$P/_log" 2>&1; local r=$?
  local got; got=$(cat "$P"/_b/*-src/origin "$P"/custom/origin 2>/dev/null | tr '\n' ' ')
  local gen; gen=$(grep -m1 -o 'ExternalProject_Add([^ ]* [^ ]*' "$P"/_b/*-download/CMakeLists.txt 2>/dev/null)
  echo "exit=$r $(grep -o 'PROBE.*' "$P/_log" | tr '\n' ' ') generated=[$gen] downloaded=[${got:-none}]"
}

echo "D1 URL: $(run d1 $W/mod/std "download_project(PROJ p1 URL $A)")"
echo "D2a BUILD_PROJECT TRUE: $(run d2a $W/mod/std "download_project(PROJ p2 URL $A BUILD_PROJECT TRUE)")"
echo "D2b BUILD_PROJECT FALSE: $(run d2b $W/mod/std "download_project(PROJ p2 URL $A BUILD_PROJECT FALSE)")"
echo "D3 PROJ twice: $(run d3 $W/mod/std "download_project(PROJ first URL $A PROJ second)")"
echo "D4 SOURCE_DIR is consumed: $(run d4 $W/mod/std "download_project(PROJ p4 URL $A SOURCE_DIR \${CMAKE_CURRENT_SOURCE_DIR}/custom)")"
echo "D5a added keyword, modified copy: $(run d5a $W/mod/extra "download_project(PROJ p5 URL $A EXTRA_KW x)")"
echo "D5b same call, unmodified copy: $(run d5b $W/mod/std "download_project(PROJ p5 URL $A EXTRA_KW x)")"
echo "D6 URL twice: $(run d6 $W/mod/std "download_project(PROJ p6 URL $A URL $B)")"
