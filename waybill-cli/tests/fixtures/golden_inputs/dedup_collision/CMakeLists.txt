# Minimal CMakeLists.txt for the dedup_collision fixture.
#
# Declares `find_package(Abseil)` so the milestone-105 find_package
# collector (cmake::collect_find_package_targets) picks up "abseil"
# as a referenced target. When US6's git-submodule reader fires
# against this fixture (post-US6), the third_party/abseil submodule
# is classified as `mikebom:build-reference: "declared-and-used"`.

cmake_minimum_required(VERSION 3.16)
project(dedup_collision_fixture LANGUAGES CXX)

find_package(Abseil REQUIRED CONFIG)
