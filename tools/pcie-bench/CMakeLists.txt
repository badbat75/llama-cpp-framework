# pcie-bench: see pcie-bench.cpp for what it measures and README.md for how to
# build and read it. Configure through build.ps1, which supplies what depends
# on this machine (compilers, HIP_PATH, the patched HIP wrapper header).
cmake_minimum_required(VERSION 3.24)
project(pcie-bench LANGUAGES CXX)

option(PCIE_BENCH_HIP  "Build pcie-bench-hip.exe (AMD, ROCm/TheRock)" ON)
option(PCIE_BENCH_CUDA "Build pcie-bench-cuda.exe (NVIDIA)"           ON)

if (PCIE_BENCH_HIP)
    # Same route as ggml-hip on Windows: the CXX compiler is ROCm's clang and
    # hip::device turns the source into a HIP one (-x hip, --offload-arch).
    # gfx1036 is the Ryzen iGPU: the HIP runtime refuses to load a module on
    # ANY device when one visible device has no code object for it.
    if (NOT GPU_TARGETS)
        set(GPU_TARGETS "gfx1201;gfx1036" CACHE STRING "HIP offload archs")
    endif()
    list(APPEND CMAKE_PREFIX_PATH "$ENV{HIP_PATH}")
    find_package(hip REQUIRED)
    add_executable(pcie-bench-hip pcie-bench.cpp)
    target_link_libraries(pcie-bench-hip PRIVATE hip::device)

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
