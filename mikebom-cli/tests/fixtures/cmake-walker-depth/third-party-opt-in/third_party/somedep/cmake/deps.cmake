# Depth-3 within third_party/. NOT walked by default (milestone 156
# FR-019). Walked only when --cmake-third-party-recursive is set OR
# MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1 env var is set.
find_package(VendoredDepDep)
