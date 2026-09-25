# pcie-bench: see pcie-bench.cpp for what it measures and README.md for how to
# build and read it. Configure through build.ps1, which supplies what depends
# on this machine (compilers, HIP_PATH, the patched HIP wrapper header).
cmake_minimum_required(VERSION 3.24)
project(pcie-bench LANGUAGES CXX)

option(PCIE_BENCH_HIP  "Build pcie-bench-hip.exe (AMD, ROCm/TheRock)" ON)
option(PCIE_BENCH_CUDA "Build pcie-bench-cuda.exe (NVIDIA)"           ON)

# Both ROCm and CUDA ship 64-bit only. A configure started by an IDE with a
# 32-bit kit does not say so: ROCm's hip-config-version.cmake rejects the
# package as "not compatible with requested version", which reads like a
# version problem. Say what it is instead.
if (NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
    message(FATAL_ERROR
        "pcie-bench needs a 64-bit toolchain, this configure is ${CMAKE_SIZEOF_VOID_P}-byte "
        "(${CMAKE_CXX_COMPILER_ID} ${CMAKE_CXX_COMPILER}). Build with build.ps1, or pick an "
        "x64 kit (for HIP: ROCm's clang++) in the IDE.")
endif()

# HIP here needs ROCm's clang as the CXX compiler (hip::device adds -x hip);
# MSVC cannot compile the kernels. Skip the HIP target rather than fail, so an
# IDE configured with cl.exe still gets the CUDA one.
if (PCIE_BENCH_HIP AND NOT CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    message(WARNING
        "PCIE_BENCH_HIP needs ROCm's clang++ as the CXX compiler, got "
        "${CMAKE_CXX_COMPILER_ID}: skipping pcie-bench-hip (build.ps1 sets it).")
    set(PCIE_BENCH_HIP OFF)
endif()

if (PCIE_BENCH_HIP)
    # Same route as ggml-hip on Windows: the CXX compiler is ROCm's clang and
    # hip::device turns the source into a HIP one (-x hip, --offload-arch).
    # gfx1036 is the Ryzen iGPU: the HIP runtime refuses to load a module on
    # ANY device when one visible device has no code object for it.
    if (NOT GPU_TARGETS)
        set(GPU_TARGETS "gfx1201;gfx1036" CACHE STRING "HIP offload archs")
    endif()
    list(APPEND CMAKE_PREFIX_PATH "$ENV{HIP_PATH}")
    find_package(hip CONFIG REQUIRED)
    add_executable(pcie-bench-hip pcie-bench.cpp)
    target_link_libraries(pcie-bench-hip PRIVATE hip::device)

    # MSVC's <cmath> clashes with the HIP device math declarations unless the
    # repo's patched __clang_hip_runtime_wrapper.h (per clang major, see
    # patches\hip\README.md) replaces the toolchain's. build.ps1 passes it in
    # CMAKE_CXX_FLAGS; a configure that did not (an IDE preset) gets it here.
    if (NOT CMAKE_CXX_FLAGS MATCHES "__CLANG_HIP_RUNTIME_WRAPPER_H__")
        string(REGEX MATCH "^[0-9]+" _clang_major "${CMAKE_CXX_COMPILER_VERSION}")
        set(_wrapper "${CMAKE_CURRENT_SOURCE_DIR}/../../patches/hip/${_clang_major}/__clang_hip_runtime_wrapper.h")
        if (NOT EXISTS "${_wrapper}")
            message(FATAL_ERROR
                "No patched HIP runtime wrapper for clang ${_clang_major} (expected ${_wrapper}): "
                "see patches/hip/README.md.")
        endif()
        target_compile_options(pcie-bench-hip PRIVATE
            -w -D__CLANG_HIP_RUNTIME_WRAPPER_H__ "SHELL:-include \"${_wrapper}\"")
    endif()

    # Stage the dist's own HIP runtime beside the exe, as the installer does
    # beside llama-server.exe: the driver's amdhip64_7.dll in System32 would
    # otherwise win the load order, and with the iGPU visible it enumerates no
    # device at all without the dist's comgr (AGENTS.md, install-runtime-deps).
    foreach (dll amdhip64_7.dll amd_comgr.dll rocm_kpack.dll)
        if (EXISTS "$ENV{HIP_PATH}/bin/${dll}")
            add_custom_command(TARGET pcie-bench-hip POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E copy_if_different "$ENV{HIP_PATH}/bin/${dll}" "$<TARGET_FILE_DIR:pcie-bench-hip>")
        endif()
    endforeach()
endif()

if (PCIE_BENCH_CUDA)
    if (NOT CMAKE_CUDA_ARCHITECTURES)
        set(CMAKE_CUDA_ARCHITECTURES native)
    endif()
    enable_language(CUDA)
    add_executable(pcie-bench-cuda pcie-bench.cu)
    set_target_properties(pcie-bench-cuda PROPERTIES CUDA_STANDARD 17)
endif()
