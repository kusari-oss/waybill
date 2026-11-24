cmake_minimum_required(VERSION 3.24)
project(hdronly VERSION 1.2.3 LANGUAGES CXX)
add_library(hdronly INTERFACE)
add_library(hdronly::hdronly ALIAS hdronly)
target_include_directories(hdronly INTERFACE
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>)
include(CMakePackageConfigHelpers)
install(TARGETS hdronly EXPORT hdronlyTargets)
install(DIRECTORY include/ DESTINATION include)
install(EXPORT hdronlyTargets NAMESPACE hdronly:: DESTINATION lib/cmake/hdronly)
file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/hdronlyConfig.cmake "include(\${CMAKE_CURRENT_LIST_DIR}/hdronlyTargets.cmake)\n")
write_basic_package_version_file(${CMAKE_CURRENT_BINARY_DIR}/hdronlyConfigVersion.cmake COMPATIBILITY SameMajorVersion)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/hdronlyConfig.cmake ${CMAKE_CURRENT_BINARY_DIR}/hdronlyConfigVersion.cmake DESTINATION lib/cmake/hdronly)
