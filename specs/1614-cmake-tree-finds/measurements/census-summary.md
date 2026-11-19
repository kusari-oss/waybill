## include() census (waybill #1605)

CMake files read on their own: 6172

| include() resolves to | calls |
|---|---|
| std-name | 320 |
| other-name | 1 |
| module | 1183 |
| builtin | 911 |
| variable | 149 |
| missing | 35 |

include() calls inside an if() block: 563

### B. Never read: 1 files reached only by other-name include()

Declarations in them: {'find': 1}

- `rocm-systems/projects/rocr-runtime/libhsakmt/CMakeLists_wsl.txt` {'find': 1} from `rocm-systems/projects/rocr-runtime/libhsakmt/CMakeLists.txt` if ['WIN_SDK']

### C. Condition lost: 19 files included only under if()

Unconditional declarations in them: {'fetch': 15, 'find': 15}

- `rocm-libraries/projects/miopen/cmake/ThirdParty.cmake` {'find': 6} from `rocm-libraries/projects/miopen/CMakeLists.txt` if ['MIOPEN_STANDALONE_BUILD']
- `rocm-systems/emulation/mirage/hotswap/cmake/Hotswap.cmake` {'fetch': 3} from `rocm-systems/emulation/mirage/CMakeLists.txt` if ['MIRAGE_BUILD_HOTSWAP']
- `compiler/amd-llvm/third-party/benchmark/cmake/llvm-toolchain.cmake` {'find': 3} from `compiler/amd-llvm/third-party/benchmark/CMakeLists.txt` if ['MSVC | else', 'BENCHMARK_ENABLE_LTO', '"${CMAKE_CXX_COMPILER_ID}" STREQUAL "GNU" | elseif']
- `rocm-systems/emulation/rocjitsu/cmake/rj_libvfio_user.cmake` {'fetch': 2} from `rocm-systems/emulation/rocjitsu/CMakeLists.txt` if ['ROCJITSU_ENABLE_VFIO']
- `compiler/amd-llvm/lldb/cmake/modules/LLDBStandalone.cmake` {'find': 2} from `compiler/amd-llvm/lldb/tools/debugserver/CMakeLists.txt` if ['CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR']; `compiler/amd-llvm/lldb/CMakeLists.txt` if ['LLDB_BUILT_STANDALONE']
- `rocm-systems/projects/rocprofiler-systems/cmake/PAPI.cmake` {'fetch': 1} from `rocm-systems/projects/rocprofiler-systems/cmake/Packages.cmake` if ['ROCPROFSYS_USE_PAPI AND ROCPROFSYS_BUILD_PAPI']
- `rocm-systems/projects/hipfile/cmake/HipFileUseGTest.cmake` {'find': 1} from `rocm-systems/projects/hipfile/CMakeLists.txt` if ['BUILD_TESTING']
- `rocm-systems/projects/clr/rocclr/cmake/ROCclrHSA.cmake` {'find': 1} from `rocm-systems/projects/clr/rocclr/cmake/ROCclr.cmake` if ['ROCCLR_ENABLE_HSA']
- `rocm-systems/projects/clr/rocclr/cmake/ROCclrPAL.cmake` {'find': 1} from `rocm-systems/projects/clr/rocclr/cmake/ROCclr.cmake` if ['ROCCLR_ENABLE_PAL']
- `rocm-libraries/projects/rocsparse/cmake/rocsparse_gtest.cmake` {'find': 1} from `rocm-libraries/projects/rocsparse/clients/CMakeLists.txt` if ['BUILD_CLIENTS_TESTS']
- `rocm-libraries/projects/rocblas/deps/external-gtest.cmake` {'fetch': 1} from `rocm-libraries/projects/rocblas/deps/CMakeLists.txt` if ['BUILD_GTEST']
- `rocm-libraries/projects/hipsolver/deps/external-gtest.cmake` {'fetch': 1} from `rocm-libraries/projects/hipsolver/deps/CMakeLists.txt` if ['BUILD_GTEST']
- `rocm-libraries/projects/hipsolver/deps/external-lapack.cmake` {'fetch': 1} from `rocm-libraries/projects/hipsolver/deps/CMakeLists.txt` if ['BUILD_LAPACK']
- `rocm-libraries/projects/hipfft/deps/external-gtest.cmake` {'fetch': 1} from `rocm-libraries/projects/hipfft/deps/CMakeLists.txt` if ['BUILD_GTEST']
- `rocm-libraries/projects/hipblaslt/deps/external-gtest.cmake` {'fetch': 1} from `rocm-libraries/projects/hipblaslt/deps/CMakeLists.txt` if ['BUILD_GTEST']
- `rocm-libraries/projects/hipblaslt/deps/external-lapack.cmake` {'fetch': 1} from `rocm-libraries/projects/hipblaslt/deps/CMakeLists.txt` if ['BUILD_LAPACK']
- `rocm-libraries/projects/hipblas/deps/external-gtest.cmake` {'fetch': 1} from `rocm-libraries/projects/hipblas/deps/CMakeLists.txt` if ['BUILD_GTEST']
- `rocm-libraries/projects/hipblas/deps/external-lapack.cmake` {'fetch': 1} from `rocm-libraries/projects/hipblas/deps/CMakeLists.txt` if ['BUILD_LAPACK']
- `compiler/amd-llvm/clang/cmake/modules/ProtobufMutator.cmake` {'fetch': 1} from `compiler/amd-llvm/lldb/tools/lldb-fuzzer/lldb-expression-fuzzer/CMakeLists.txt` if ['CLANG_ENABLE_PROTO_FUZZER', 'TARGET lldb-expression-fuzzer']; `compiler/amd-llvm/clang/tools/clang-fuzzer/CMakeLists.txt` if ['CLANG_ENABLE_PROTO_FUZZER']

### E. Package names declared only under conditions

| kind | names | only conditional, in-file alone | only conditional, effective |
|---|---|---|---|
| find | 225 | 109 | 131 |
| fetch | 55 | 34 | 44 |

### F. find_package names by class (CMake modules: 162 Find*.cmake in /usr/local/share/cmake-3.31/Modules)

| class | names | only conditional, effective | examples |
|---|---|---|---|
| first-party-project | 63 | 25 | hip, rocprofiler-sdk, gtest, rocprofiler-register, llvm, rocm, rocprim, rocdecode, amd_comgr, clang, rocrand, rocjpeg, rocthrust, rocblas |
| first-party-config | 25 | 8 | rocmcmakebuildtools, openmp, hsa-runtime64, mpi, amd_smi, hiprtc, numa, sqlite3, libdw, libelf, amddevicelibs, hsakmt, zstd, hipdnn_integration_tests |
| project-find-module | 45 | 33 | ffmpeg, lapack, blas, rocprofiler_sdk_pytest, blis, libxml2, amd_hsa_loader, cholmod, ffi, fftw, grpc, isl, libdrm_amdgpu, libedit |
| cmake-find-module | 26 | 17 | threads, python3, git, pkgconfig, cudatoolkit, python, cuda, doxygen, protobuf, backtrace, opengl, boost, curl, glew |
| other | 66 | 48 | rocprofiler-sdk-roctx, cccl, nanobind, drm, drm_amdgpu, fmt, catch2, msgpack-cxx, tbb, msgpackc-cxx, nlohmann_json, opencv, nvhpc, openblas |

### D. Context: 175 conditional add_subdirectory() with declarations beneath

Unconditional declarations beneath them (approximate): {'find': 669, 'fetch': 7}

- `rocm-systems/projects/rocprofiler-sdk/tests` {'find': 253} if ['ROCPROFILER_BUILD_TESTS AND ROCPROFILER_BUILD_INTEGRATION_TESTS'] (from `rocm-systems/projects/rocprofiler-sdk/CMakeLists.txt`:143)
- `rocm-systems/projects/rocprofiler-systems/examples` {'find': 37} if ['ROCPROFSYS_BUILD_EXAMPLES'] (from `rocm-systems/projects/rocprofiler-systems/CMakeLists.txt`:618)
- `rocm-systems/projects/rocprofiler-sdk/samples` {'find': 30} if ['ROCPROFILER_BUILD_SAMPLES'] (from `rocm-systems/projects/rocprofiler-sdk/CMakeLists.txt`:147)
- `base` {'find': 18} if ['THEROCK_ENABLE_BASE'] (from `CMakeLists.txt`:506)
- `rocm-systems/projects/rocprofiler/tests-v2` {'find': 12} if ['ROCPROFILER_BUILD_TESTS'] (from `rocm-systems/projects/rocprofiler/CMakeLists.txt`:220)
- `rocm-libraries/projects/hipccl/hipccl2` {'find': 11} if ['HIPCCL_BUILD_LEGACY'] (from `rocm-libraries/projects/hipccl/CMakeLists.txt`:63)
- `rocm-libraries/projects/hipccl/hipccl3` {'find': 11} if ['HIPCCL_BUILD_LEGACY | else'] (from `rocm-libraries/projects/hipccl/CMakeLists.txt`:66)
- `rocm-libraries/shared/rocroller/test` {'find': 10} if ['ROCROLLER_BUILD_TESTING'] (from `rocm-libraries/shared/rocroller/CMakeLists.txt`:280)
- `rocm-libraries/projects/rocblas/next-cmake/clients` {'find': 6} if ['ROCBLAS_ENABLE_CLIENT'] (from `rocm-libraries/projects/rocblas/next-cmake/CMakeLists.txt`:127)
- `rocm-systems/projects/clr/rocclr` {'find': 6} if ['(CLR_BUILD_HIP AND HIP_PLATFORM STREQUAL "amd") OR CLR_BUILD_OCL'] (from `rocm-systems/projects/clr/CMakeLists.txt`:102)
- `rocm-libraries/dnn-providers/hip-kernel-provider/rocke` {'find': 5} if ['HIPKERNELPROVIDER_ENABLE_ROCKE'] (from `rocm-libraries/dnn-providers/hip-kernel-provider/CMakeLists.txt`:532)
- `rocm-libraries/projects/hipfft/clients` {'find': 5} if ['BUILD_CLIENTS_BENCH OR BUILD_CLIENTS_SAMPLES OR BUILD_CLIENTS_TESTS'] (from `rocm-libraries/projects/hipfft/CMakeLists.txt`:262)
- `rocm-libraries/projects/hipfft/clients/tests` {'find': 5} if ['BUILD_CLIENTS_TESTS'] (from `rocm-libraries/projects/hipfft/clients/CMakeLists.txt`:113)
- `rocm-libraries/shared/rocroller/test/rocprofiler` {'find': 5} if ['ROCROLLER_ENABLE_CATCH', 'ROCROLLER_ENABLE_ROCPROF_ANALYZER'] (from `rocm-libraries/shared/rocroller/test/CMakeLists.txt`:106)
- `rocm-systems/projects/hrr/tests` {'find': 5} if ['HRR_BUILD_TESTS AND EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/tests/CMakeLists.txt"'] (from `rocm-systems/projects/hrr/CMakeLists.txt`:45)
- `rocm-systems/projects/rocdecode/test` {'find': 5} if ['_deps_met'] (from `rocm-systems/projects/rocdecode/CMakeLists.txt`:508)
- `rocm-libraries/projects/composablekernel/codegen` {'find': 4} if ['CK_USE_CODEGEN AND (SUPPORTED_GPU_TARGETS MATCHES "gfx9" OR GPU_ARCHS)'] (from `rocm-libraries/projects/composablekernel/CMakeLists.txt`:1014)
- `rocm-libraries/projects/hipdnn/backend` {'find': 4} if ['HIPDNN_BUILD_BACKEND'] (from `rocm-libraries/projects/hipdnn/CMakeLists.txt`:319)
- `rocm-libraries/projects/rocfft/clients` {'find': 4} if ['BUILD_CLIENTS_SAMPLES OR BUILD_CLIENTS_TESTS OR BUILD_CLIENTS_BENCH'] (from `rocm-libraries/projects/rocfft/CMakeLists.txt`:285)
- `rocm-libraries/projects/rocwmma/test` {'find': 4} if ['ROCWMMA_BUILD_TESTS'] (from `rocm-libraries/projects/rocwmma/CMakeLists.txt`:258)
- `rocm-systems/projects/rocprofiler/samples` {'find': 4} if ['ROCPROFILER_BUILD_SAMPLES'] (from `rocm-systems/projects/rocprofiler/CMakeLists.txt`:214)
- `rocm-systems/projects/rocprofiler/test` {'find': 4} if ['ROCPROFILER_BUILD_TESTS'] (from `rocm-systems/projects/rocprofiler/CMakeLists.txt`:219)
- `rocm-systems/projects/rocprofiler-register/tests` {'find': 4} if ['ROCPROFILER_REGISTER_BUILD_TESTS'] (from `rocm-systems/projects/rocprofiler-register/CMakeLists.txt`:109)
- `rocm-systems/projects/rocprofiler-sdk/tests/bin/rocshmem` {'find': 4} if ['rocshmem_FOUND AND rocshmem_VERSION VERSION_GREATER_EQUAL 3.3.0'] (from `rocm-systems/projects/rocprofiler-sdk/tests/bin/CMakeLists.txt`:66)
- `rocm-systems/projects/rocr-debug-agent/test` {'find': 4} if ['ENABLE_TESTS'] (from `rocm-systems/projects/rocr-debug-agent/CMakeLists.txt`:256)
