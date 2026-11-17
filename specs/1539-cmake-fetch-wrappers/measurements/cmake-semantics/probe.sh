#!/bin/bash
# Spec 1539 research: how CMake binds a call to a fetch wrapper, measured
# with the installed cmake. No network: every URL is a file:// tarball
# built here, each holding one file `origin` that names it.
# Usage: probe.sh <scratch-dir>. Prints one line per probe.
set -u
W=$1; rm -rf "$W"; mkdir -p "$W/tars"
for t in a b c; do mkdir -p "$W/tars/$t"; echo "$t" > "$W/tars/$t/origin"; tar -C "$W/tars/$t" -czf "$W/tars/$t-1.0.tar.gz" origin; done
# Served over localhost HTTP: ExternalProject rejects a list of file:// URLs
# ("At least one entry of URL is a path"), and a repeated URL is a list.
PORT=${PORT:-18539}
python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$W/tars" > "$W/http.log" 2>&1 &
HTTP=$!; trap 'kill $HTTP 2>/dev/null' EXIT; sleep 1
A=http://127.0.0.1:$PORT/a-1.0.tar.gz; B=http://127.0.0.1:$PORT/b-1.0.tar.gz
HDR='cmake_minimum_required(VERSION 3.25)'
# run <dir> [target]: configure, optionally build a target; print the log's PROBE lines.
run() {
  cmake -S "$1" -B "$1/_b" -Wno-dev > "$1/_log" 2>&1; local r=$?
  if [ $r -eq 0 ] && [ -n "${2:-}" ]; then cmake --build "$1/_b" --target "$2" >> "$1/_log" 2>&1; r=$?; fi
  echo "exit=$r $(grep -o 'PROBE[^"]*' "$1/_log" | tr '\n' ' ')"
}
# ep_origin <dir> <name>: which tarball ExternalProject <name> downloaded.
ep_origin() { cat "$1/_b/$2-prefix/src/$2/origin" 2>/dev/null || echo none; }
fc_origin() { cat "$1/_b/_deps/$2-src/origin" 2>/dev/null || echo none; }

# W1: TheRock's shape. PARSE_ARGV 1 with URL/URL_HASH not among the keywords:
# what reaches ExternalProject_Add through ${ARG_UNPARSED_ARGUMENTS}?
P=$W/w1; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w1 NONE)
include(ExternalProject)
function(wf target_name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "CMAKE_PROJECT" "SOURCE_DIR;PREFIX" "TOUCH")
  message(STATUS "PROBE unparsed=[\${ARG_UNPARSED_ARGUMENTS}] touch=[\${ARG_TOUCH}] source_dir=[\${ARG_SOURCE_DIR}]")
  ExternalProject_Add("\${target_name}" CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "" \${ARG_UNPARSED_ARGUMENTS})
  get_property(u TARGET \${target_name} PROPERTY _EP_URL)
  message(STATUS "PROBE _EP_URL=[\${u}]")
endfunction()
wf(wz TOUCH t1 t2 URL "$A" URL_HASH "SHA256=00" EXTRA)
C
echo "W1 a multi-value keyword before URL: $(run $P)"
# W1b: the same wrapper, called in TheRock's argument order (TOUCH last).
P=$W/w1b; mkdir -p $P; sed 's/^wf(wz .*/wf(wz SOURCE_DIR "${CMAKE_CURRENT_BINARY_DIR}\/wz-src" URL "'"${A//\//\\/}"'" TOUCH t1)/' $W/w1/CMakeLists.txt > $P/CMakeLists.txt
echo "W1b TheRock's order (SOURCE_DIR, URL, TOUCH last): $(run $P wz) downloaded=$(ep_origin $P wz) (SOURCE_DIR is parsed and not re-passed, so the default source dir is used)"

# W2: a keyword given twice to ExternalProject_Add (body literal, then the
# caller's through ARGN). Which URL is downloaded?
P=$W/w2; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w2 NONE)
include(ExternalProject)
function(wf name)
  ExternalProject_Add(\${name} URL "$A" CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "" \${ARGN})
  get_property(u TARGET \${name} PROPERTY _EP_URL)
  message(STATUS "PROBE _EP_URL=[\${u}]")
endfunction()
wf(e2 URL "$B")
C
echo "W2 ExternalProject_Add URL twice (body a, caller b): $(run $P e2) downloaded=$(ep_origin $P e2)"

# W2b: URL with two values in one keyword (EP documents URL as a list).
P=$W/w2b; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w2b NONE)
include(ExternalProject)
ExternalProject_Add(e2b URL "$A" "$B" CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
get_property(u TARGET e2b PROPERTY _EP_URL)
message(STATUS "PROBE _EP_URL=[\${u}]")
C
echo "W2b ExternalProject_Add URL a b: $(run $P e2b) downloaded=$(ep_origin $P e2b)"

# W3: a keyword given twice to FetchContent_Declare.
P=$W/w3; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w3 NONE)
include(FetchContent)
macro(wf name)
  FetchContent_Declare(\${name} URL "$A" \${ARGN})
endmacro()
wf(f3 URL "$B")
FetchContent_MakeAvailable(f3)
message(STATUS "PROBE populated=\${f3_POPULATED}")
C
echo "W3 FetchContent_Declare URL twice (body a, caller b): $(run $P) downloaded=$(fc_origin $P f3)"

# W4: a one-value keyword given twice to cmake_parse_arguments.
P=$W/w4; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w4 NONE)
function(wf)
  cmake_parse_arguments(PARSE_ARGV 0 W "" "URL" "")
  message(STATUS "PROBE W_URL=[\${W_URL}] unparsed=[\${W_UNPARSED_ARGUMENTS}]")
endfunction()
wf(URL first URL second)
C
echo "W4 cmake_parse_arguments one-value keyword twice: $(run $P)"

# W5: a wrapper defined twice. Which body runs depends on evaluation order.
P=$W/w5; mkdir -p $P/x $P/y
printf 'function(wf)\n  message(STATUS "PROBE body=x")\nendfunction()\n' > $P/x/def.cmake
printf 'function(wf)\n  message(STATUS "PROBE body=y")\nendfunction()\n' > $P/y/def.cmake
printf '%s\nproject(w5 NONE)\ninclude(x/def.cmake)\nwf()\ninclude(y/def.cmake)\nwf()\n' "$HDR" > $P/CMakeLists.txt
echo "W5 defined twice, called after each: $(run $P)"
P=$W/w5b; mkdir -p $P/sub; cp -R $W/w5/x $W/w5/y $P/
printf '%s\nproject(w5b NONE)\ninclude(x/def.cmake)\nadd_subdirectory(sub)\nwf()\n' "$HDR" > $P/CMakeLists.txt
printf 'include(${CMAKE_SOURCE_DIR}/y/def.cmake)\nwf()\n' > $P/sub/CMakeLists.txt
echo "W5b redefined in a subdirectory, then called in the parent: $(run $P)"

# W6: quoted "${ARG_UNPARSED_ARGUMENTS}" passes one argument.
P=$W/w6; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w6 NONE)
include(ExternalProject)
function(wf name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "" "" "")
  ExternalProject_Add(\${name} CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "" "\${ARG_UNPARSED_ARGUMENTS}")
  get_property(u TARGET \${name} PROPERTY _EP_URL)
  message(STATUS "PROBE _EP_URL=[\${u}]")
endfunction()
wf(e6 URL "$A")
C
echo "W6 quoted unparsed forwarding: $(run $P)"

# W7: macro vs function: ${ARGN} and ${ARGV1} in a macro body.
P=$W/w7; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w7 NONE)
macro(wm name)
  message(STATUS "PROBE macro name=\${name} argv1=\${ARGV1} argn=[\${ARGN}]")
endmacro()
function(wfn name)
  message(STATUS "PROBE function name=\${name} argv1=\${ARGV1} argn=[\${ARGN}]")
endfunction()
wm(n1 URL u1)
wfn(n1 URL u1)
C
echo "W7 macro and function argument variables: $(run $P)"

# W8: a default the body sets for a parsed variable the call did not pass.
P=$W/w8; mkdir -p $P
cat > $P/CMakeLists.txt <<C
$HDR
project(w8 NONE)
include(ExternalProject)
function(wf name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "" "URL" "")
  if(NOT DEFINED ARG_URL)
    set(ARG_URL "$A")
  endif()
  ExternalProject_Add(\${name} URL \${ARG_URL} CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "")
endfunction()
wf(e8a)
wf(e8b URL "$B")
C
echo "W8 body default: $(run $P e8a) $(cmake --build $P/_b --target e8b >/dev/null 2>&1; echo) e8a=$(ep_origin $P e8a) e8b=$(ep_origin $P e8b)"
