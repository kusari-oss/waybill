cmake_minimum_required(VERSION 3.24)
project(mixedcase LANGUAGES CXX)
add_library(mixedcase_lib STATIC mixedcase.cpp)
target_include_directories(mixedcase_lib PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
