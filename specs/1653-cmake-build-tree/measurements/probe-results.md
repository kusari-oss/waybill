# Probe results

## versions.txt

```
apt cmake version 3.25.1
pip3.31 cmake version 3.31.6
pip4.0 cmake version 4.0.3
ninja 1.11.1
make GNU Make 4.3
cxx c++ (Debian 12.2.0-14+deb12u1) 12.2.0
```

## image.txt

```
debian@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e
```

## runs.txt

```
apt-ninja-fetch configure=0 build=0
apt-ninja-system configure=0 build=0
apt-ninja-confonly configure=0 build=skipped
apt-ninja-failed configure=0 build=1
apt-make-fetch configure=0 build=0
apt-make-system configure=0 build=0
apt-make-confonly configure=0 build=skipped
apt-make-failed configure=0 build=2
pip3.31-ninja-fetch configure=0 build=0
pip3.31-ninja-system configure=0 build=0
pip3.31-ninja-confonly configure=0 build=skipped
pip3.31-ninja-failed configure=0 build=1
pip3.31-make-fetch configure=0 build=0
pip3.31-make-system configure=0 build=0
pip3.31-make-confonly configure=0 build=skipped
pip3.31-make-failed configure=0 build=2
pip4.0-ninja-fetch configure=0 build=0
pip4.0-ninja-system configure=0 build=0
pip4.0-ninja-confonly configure=0 build=skipped
pip4.0-ninja-failed configure=0 build=1
pip4.0-make-fetch configure=0 build=0
pip4.0-make-system configure=0 build=0
pip4.0-make-confonly configure=0 build=skipped
pip4.0-make-failed configure=0 build=2
```

## Signals per configuration and dependency

One line per distinct signal set; `runs` lists which version/generator pairs gave it.

- **confonly** ExternalProject `ExtMixed`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 6/6)
- **confonly** ExternalProject `extgit`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 6/6)
- **confonly** ExternalProject `extproj`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 6/6)
- **confonly** FetchContent `bdep`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **confonly** FetchContent `extra`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **confonly** FetchContent `hdronly`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=redirect, populate-done=True, src=True  (runs: 6/6)
- **confonly** FetchContent `mixedcase`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **confonly** FetchContent `neverused`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **confonly** FetchContent `urldep`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **failed** ExternalProject `ExtMixed`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 6/6)
- **failed** ExternalProject `extgit`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 3/6: apt/make pip3.31/make pip4.0/make)
- **failed** ExternalProject `extgit`: done=False, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 3/6: apt/ninja pip3.31/ninja pip4.0/ninja)
- **failed** ExternalProject `extproj`: done=False, download=False, prefix=True, src-nonempty=False, stamp-dir=True  (runs: 6/6)
- **failed** FetchContent `bdep`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **failed** FetchContent `extra`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **failed** FetchContent `hdronly`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=redirect, populate-done=True, src=True  (runs: 6/6)
- **failed** FetchContent `mixedcase`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **failed** FetchContent `neverused`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **failed** FetchContent `urldep`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **fetch** ExternalProject `ExtMixed`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **fetch** ExternalProject `extgit`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **fetch** ExternalProject `extproj`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **fetch** FetchContent `bdep`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **fetch** FetchContent `extra`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **fetch** FetchContent `hdronly`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=redirect, populate-done=True, src=True  (runs: 6/6)
- **fetch** FetchContent `mixedcase`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **fetch** FetchContent `neverused`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **fetch** FetchContent `urldep`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **system** ExternalProject `ExtMixed`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **system** ExternalProject `extgit`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **system** ExternalProject `extproj`: done=True, download=True, prefix=True, src-nonempty=True, stamp-dir=True  (runs: 6/6)
- **system** FetchContent `bdep`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **system** FetchContent `extra`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **system** FetchContent `hdronly`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=system, populate-done=False, src=False  (runs: 6/6)
- **system** FetchContent `mixedcase`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)
- **system** FetchContent `neverused`: FC_SOURCE_DIR_entry=False, SOURCE_DIR=False, _DIR=-, populate-done=False, src=False  (runs: 6/6)
- **system** FetchContent `urldep`: FC_SOURCE_DIR_entry=True, SOURCE_DIR=True, _DIR=-, populate-done=True, src=True  (runs: 6/6)

## Build-tree size

| run | entries | bytes (files) | file-API reply |
|---|---|---|---|
| apt-make-confonly | 343 | 525756 | no |
| apt-make-failed | 348 | 524199 | no |
| apt-make-fetch | 563 | 1351565 | no |
| apt-make-system | 610 | 1430256 | yes |
| apt-ninja-confonly | 256 | 451171 | no |
| apt-ninja-failed | 322 | 485697 | no |
| apt-ninja-fetch | 436 | 1257956 | no |
| apt-ninja-system | 449 | 1310099 | yes |
| pip3.31-make-confonly | 355 | 584746 | no |
| pip3.31-make-failed | 360 | 583107 | no |
| pip3.31-make-fetch | 575 | 1462834 | no |
| pip3.31-make-system | 624 | 1550307 | yes |
| pip3.31-ninja-confonly | 268 | 507723 | no |
| pip3.31-ninja-failed | 332 | 539335 | no |
| pip3.31-ninja-fetch | 448 | 1377606 | no |
| pip3.31-ninja-system | 463 | 1436211 | yes |
| pip4.0-make-confonly | 359 | 584610 | no |
| pip4.0-make-failed | 364 | 582957 | no |
| pip4.0-make-fetch | 582 | 1463390 | no |
| pip4.0-make-system | 632 | 1550488 | yes |
| pip4.0-ninja-confonly | 272 | 508382 | no |
| pip4.0-ninja-failed | 336 | 539949 | no |
| pip4.0-ninja-fetch | 455 | 1379873 | no |
| pip4.0-ninja-system | 471 | 1438336 | yes |

## compile_commands.json

- apt-make-confonly: 3 entries; include dirs: /work/b/apt-make-confonly/_deps/hdronly-src/include, /work/b/apt-make-confonly/_deps/mixedcase-src, /work/b/apt-make-confonly/_deps/urldep-src
- apt-make-failed: 3 entries; include dirs: /work/b/apt-make-failed/_deps/hdronly-src/include, /work/b/apt-make-failed/_deps/mixedcase-src, /work/b/apt-make-failed/_deps/urldep-src
- apt-make-fetch: 3 entries; include dirs: /work/b/apt-make-fetch/_deps/hdronly-src/include, /work/b/apt-make-fetch/_deps/mixedcase-src, /work/b/apt-make-fetch/_deps/urldep-src
- apt-make-system: 5 entries; include dirs: /work/b/apt-make-system/_deps/bdep-src, /work/b/apt-make-system/_deps/extra-src, /work/b/apt-make-system/_deps/mixedcase-src, /work/b/apt-make-system/_deps/urldep-src, /work/prefix-apt/include
- apt-ninja-confonly: 3 entries; include dirs: /work/b/apt-ninja-confonly/_deps/hdronly-src/include, /work/b/apt-ninja-confonly/_deps/mixedcase-src, /work/b/apt-ninja-confonly/_deps/urldep-src
- apt-ninja-failed: 3 entries; include dirs: /work/b/apt-ninja-failed/_deps/hdronly-src/include, /work/b/apt-ninja-failed/_deps/mixedcase-src, /work/b/apt-ninja-failed/_deps/urldep-src
- apt-ninja-fetch: 3 entries; include dirs: /work/b/apt-ninja-fetch/_deps/hdronly-src/include, /work/b/apt-ninja-fetch/_deps/mixedcase-src, /work/b/apt-ninja-fetch/_deps/urldep-src
- apt-ninja-system: 5 entries; include dirs: /work/b/apt-ninja-system/_deps/bdep-src, /work/b/apt-ninja-system/_deps/extra-src, /work/b/apt-ninja-system/_deps/mixedcase-src, /work/b/apt-ninja-system/_deps/urldep-src, /work/prefix-apt/include
- pip3.31-make-confonly: 3 entries; include dirs: /work/b/pip3.31-make-confonly/_deps/hdronly-src/include, /work/b/pip3.31-make-confonly/_deps/mixedcase-src, /work/b/pip3.31-make-confonly/_deps/urldep-src
- pip3.31-make-failed: 3 entries; include dirs: /work/b/pip3.31-make-failed/_deps/hdronly-src/include, /work/b/pip3.31-make-failed/_deps/mixedcase-src, /work/b/pip3.31-make-failed/_deps/urldep-src
- pip3.31-make-fetch: 3 entries; include dirs: /work/b/pip3.31-make-fetch/_deps/hdronly-src/include, /work/b/pip3.31-make-fetch/_deps/mixedcase-src, /work/b/pip3.31-make-fetch/_deps/urldep-src
- pip3.31-make-system: 5 entries; include dirs: /work/b/pip3.31-make-system/_deps/bdep-src, /work/b/pip3.31-make-system/_deps/extra-src, /work/b/pip3.31-make-system/_deps/mixedcase-src, /work/b/pip3.31-make-system/_deps/urldep-src, /work/prefix-pip3.31/include
- pip3.31-ninja-confonly: 3 entries; include dirs: /work/b/pip3.31-ninja-confonly/_deps/hdronly-src/include, /work/b/pip3.31-ninja-confonly/_deps/mixedcase-src, /work/b/pip3.31-ninja-confonly/_deps/urldep-src
- pip3.31-ninja-failed: 3 entries; include dirs: /work/b/pip3.31-ninja-failed/_deps/hdronly-src/include, /work/b/pip3.31-ninja-failed/_deps/mixedcase-src, /work/b/pip3.31-ninja-failed/_deps/urldep-src
- pip3.31-ninja-fetch: 3 entries; include dirs: /work/b/pip3.31-ninja-fetch/_deps/hdronly-src/include, /work/b/pip3.31-ninja-fetch/_deps/mixedcase-src, /work/b/pip3.31-ninja-fetch/_deps/urldep-src
- pip3.31-ninja-system: 5 entries; include dirs: /work/b/pip3.31-ninja-system/_deps/bdep-src, /work/b/pip3.31-ninja-system/_deps/extra-src, /work/b/pip3.31-ninja-system/_deps/mixedcase-src, /work/b/pip3.31-ninja-system/_deps/urldep-src, /work/prefix-pip3.31/include
- pip4.0-make-confonly: 3 entries; include dirs: /work/b/pip4.0-make-confonly/_deps/hdronly-src/include, /work/b/pip4.0-make-confonly/_deps/mixedcase-src, /work/b/pip4.0-make-confonly/_deps/urldep-src
- pip4.0-make-failed: 3 entries; include dirs: /work/b/pip4.0-make-failed/_deps/hdronly-src/include, /work/b/pip4.0-make-failed/_deps/mixedcase-src, /work/b/pip4.0-make-failed/_deps/urldep-src
- pip4.0-make-fetch: 3 entries; include dirs: /work/b/pip4.0-make-fetch/_deps/hdronly-src/include, /work/b/pip4.0-make-fetch/_deps/mixedcase-src, /work/b/pip4.0-make-fetch/_deps/urldep-src
- pip4.0-make-system: 5 entries; include dirs: /work/b/pip4.0-make-system/_deps/bdep-src, /work/b/pip4.0-make-system/_deps/extra-src, /work/b/pip4.0-make-system/_deps/mixedcase-src, /work/b/pip4.0-make-system/_deps/urldep-src, /work/prefix-pip4.0/include
- pip4.0-ninja-confonly: 3 entries; include dirs: /work/b/pip4.0-ninja-confonly/_deps/hdronly-src/include, /work/b/pip4.0-ninja-confonly/_deps/mixedcase-src, /work/b/pip4.0-ninja-confonly/_deps/urldep-src
- pip4.0-ninja-failed: 3 entries; include dirs: /work/b/pip4.0-ninja-failed/_deps/hdronly-src/include, /work/b/pip4.0-ninja-failed/_deps/mixedcase-src, /work/b/pip4.0-ninja-failed/_deps/urldep-src
- pip4.0-ninja-fetch: 3 entries; include dirs: /work/b/pip4.0-ninja-fetch/_deps/hdronly-src/include, /work/b/pip4.0-ninja-fetch/_deps/mixedcase-src, /work/b/pip4.0-ninja-fetch/_deps/urldep-src
- pip4.0-ninja-system: 5 entries; include dirs: /work/b/pip4.0-ninja-system/_deps/bdep-src, /work/b/pip4.0-ninja-system/_deps/extra-src, /work/b/pip4.0-ninja-system/_deps/mixedcase-src, /work/b/pip4.0-ninja-system/_deps/urldep-src, /work/prefix-pip4.0/include
