# TheRock (SC-003)

Measured in CI only, never locally: public-corpus run 43676912729, on branch `1530-therock-capture` at `2562e5c0`. That branch is `1530-git-submodule-reader` plus one workflow commit adding a measurement step, and is never merged. The clone was made without `--recurse-submodules`, at `b1cae390cc745283dce25581415353f9bc906162`, and scanned with the branch's release binary under `--offline`.

| measure | count |
| --- | --- |
| gitlinks (`git ls-tree -r HEAD`, mode 160000) | 11 |
| `path =` lines in `.gitmodules` | 11 |
| `git-submodule` components | 11 |
| C222 | `{"pinned":11,"skipped":{"git-refused":0,"git-unavailable":0,"no-gitlink":0,"no-repository-data":0,"no-url":0,"unsafe-path-or-url":0}}` |

**SC-003 holds**: components equal gitlinks, and nothing is skipped. All 11 are `pkg:github/rocm/…`: ROCm's organisation is `ROCm`, lowercased per the `github` type definition (FR-004).

#1515's figure of 62 paths for 36 gitlinks was measured on a tree with its submodules initialised recursively, so nested `.gitmodules` files existed. This clone initialises none, so only the top level's 11 gitlinks are visible, as FR-006 requires.

**C57.** All 11 are `declared-only`. TheRock consumes its submodules through its own CMake functions, not `add_subdirectory` or `find_package`, and the CMake collectors see only the files `discover_cmake_files` returns (#1516). Recognising TheRock's functions is a TheRock-cataloger concern (#1539), recorded for #1545.

Gitlinks (path, commit):

```
base/half	207ee58595a64b5c4a70df221f1e6e704b807811
base/rocm-cmake	10155d7272ea1bf79f6b5a9dbc339657af1aa372
compiler/amd-llvm	4b5cc0619e61b882af37dc5b73504e10e2515b5d
compiler/hipify	d364ef407b2c3e20c6ceb3575bd852f0c7cebb8c
compiler/spirv-llvm-translator	ff3483afbc6ff3e200c4d51e51c501186d6b1d18
debug-tools/rocgdb/source	b79d66da2acad573ed4536d0ad1f042c48a98c69
math-libs/libhipcxx	88f23f01fd7000c1ad25492311693e8e7ccabe31
rocm-docs	9bcf25db1e77f58b435fe162baaa99566bac0189
rocm-libraries	98e132f8d03e9e09441b32db692bdfc8c714563a
rocm-systems	c1ab6df5e4180e4f69d60ae1ae046fa390e22a87
third-party/sysdeps/common/mesa-fork	4b0d67e19365c50d1a60e8ada444323c4c82c458
```

Components (PURL, C57):

```
pkg:github/rocm/half@207ee58595a64b5c4a70df221f1e6e704b807811	declared-only
pkg:github/rocm/hipify@d364ef407b2c3e20c6ceb3575bd852f0c7cebb8c	declared-only
pkg:github/rocm/libhipcxx@88f23f01fd7000c1ad25492311693e8e7ccabe31	declared-only
pkg:github/rocm/llvm-project@4b5cc0619e61b882af37dc5b73504e10e2515b5d	declared-only
pkg:github/rocm/mesa-fork@4b0d67e19365c50d1a60e8ada444323c4c82c458	declared-only
pkg:github/rocm/rocgdb@b79d66da2acad573ed4536d0ad1f042c48a98c69	declared-only
pkg:github/rocm/rocm-cmake@10155d7272ea1bf79f6b5a9dbc339657af1aa372	declared-only
pkg:github/rocm/rocm-docs@9bcf25db1e77f58b435fe162baaa99566bac0189	declared-only
pkg:github/rocm/rocm-libraries@98e132f8d03e9e09441b32db692bdfc8c714563a	declared-only
pkg:github/rocm/rocm-systems@c1ab6df5e4180e4f69d60ae1ae046fa390e22a87	declared-only
pkg:github/rocm/spirv-llvm-translator@ff3483afbc6ff3e200c4d51e51c501186d6b1d18	declared-only
```
