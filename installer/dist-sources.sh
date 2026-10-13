# Linux counterpart of dist-sources.psd1: where the external toolchain
# distributions come from and where they are installed. Sources only, NO
# versions: every reader resolves the newest STABLE release at run time.
# Sourced (never executed) by common.sh, i.e. by every *.sh build script.
#
# Layout: one directory per version under each root, and under the version one
# directory per OS, <Root>/<Version>/<DIST_OS>, e.g.
# /mnt/data/TheRock/10.1.0/linux. The drive is shared with the Windows side of
# the same machine, so a Windows dist of the same version sits beside it as
# <Root>/<Version>/windows (dist-sources.psd1 DistOs, installed by
# 00-install-prerequisites.ps1) and neither build ever sees the other's files. Dists
# live side by side: an upgrade lands next to its predecessor, and nothing
# here ever deletes one (that call is the user's).
#
# Nothing is exported machine-wide: no profile script, no /etc/environment.
# The build scripts resolve the newest complete dist under each root on every
# run, so a version bump needs no environment change.

# The dist root, the counterpart of DistRoot in dist-sources.psd1. /mnt/data
# is the DEFAULT; it is overridden, in this order, by --dist-root on
# 00-install-prerequisites.sh or 01-configure.sh (DIST_ROOT_OVERRIDE, set by
# use_dist_root in common.sh, which sources this file again), by the
# LLAMA_FW_DIST_ROOT environment variable, and by the CFG_DIST_ROOT that
# 01-configure.sh recorded in build/linux/config-build.sh. 02-build.sh and
# 03-package.sh use the recorded one. Assignments only, so sourcing it twice
# is safe.
DIST_ROOT="${DIST_ROOT_OVERRIDE:-${LLAMA_FW_DIST_ROOT:-${CFG_DIST_ROOT:-/mnt/data}}}"
DIST_OS='linux'

# AMD ROCm/HIP: TheRock dist tarball. Multiarch on purpose, as on Windows:
# kernels for every gfx family, so one install serves whatever GPU the machine
# has. The newest stable is FOUND in the indexes, never composed (the host
# depends on the version: see dist-sources.psd1), matched strictly as
# therock-dist-linux-<flavor>-<X.Y.Z>.tar.gz so `-tests-` builds and dated
# nightlies are never taken as releases. ~9.5 GB download, ~22 GB on disk.
ROCM_ROOT="$DIST_ROOT/TheRock"
ROCM_MARKER='.therock-version'      # written only after a successful extract
ROCM_FLAVOR='multiarch'
ROCM_INDEXES=(
    'https://stable.repo.amd.com/rocm/core/tarball/'    # 10.0 onwards
    'https://repo.amd.com/rocm/tarball-multi-arch/'     # up to 7.14
)

# NVIDIA CUDA Toolkit, assembled from the REDIST archives rather than the
# .run installer or the apt repo: plain tarballs, no root, no display driver
# touched, any install prefix. CUDA_REDIST_INDEX lists one
# redistrib_<X.Y.Z>.json per toolkit release, each naming every component's
# archive path and sha256; the highest X.Y.Z is the newest toolkit. The
# components are what ggml-cuda and CMake's FindCUDAToolkit use (compiler,
# cudart, cuBLAS, CCCL) plus the small tools around them, the same selection
# as the Windows Components list. They are merged into ONE prefix,
# <Root>/<X.Y.Z>/linux, which then looks like a regular toolkit tree.
CUDA_ROOT="$DIST_ROOT/CUDA"
CUDA_MARKER='.cuda-version'
CUDA_REDIST_INDEX='https://developer.download.nvidia.com/compute/cuda/redist/'
CUDA_PLATFORM='linux-x86_64'
CUDA_COMPONENTS=(
    cuda_nvcc cuda_crt libnvvm libnvptxcompiler libnvfatbin libnvjitlink
    cuda_cudart cuda_culibos libcublas cccl cuda_nvrtc cuda_nvtx
    cuda_profiler_api cuda_cuobjdump cuda_nvdisasm cuda_cuxxfilt
    cuda_nvprune cuda_nvml_dev
)

# NVIDIA NCCL, the collective library `--split-mode tensor` reduces through
# between two or more CUDA devices (Linux only: there is no NCCL for Windows).
# NVIDIA keeps it OUT of the toolkit redist above and publishes no archive
# index for it; the forms it ships in are an apt repo per distro release and
# the PyPI wheel, and the wheel is the one that needs no root and no distro
# match: a zip holding libnccl.so.2 and the headers, with its version and
# sha256 in PyPI's JSON API. The project is named after the CUDA MAJOR it is
# built for (nvidia-nccl-cu13), so NCCL_PYPI_JSON takes that major, read from
# the installed toolkit. Unpacked to <Root>/<X.Y.Z>/linux/{lib,include}.
NCCL_ROOT="$DIST_ROOT/NCCL"
NCCL_MARKER='.nccl-version'         # "<X.Y.Z> cu<major>", written last
NCCL_PYPI_JSON='https://pypi.org/pypi/nvidia-nccl-cu%s/json'
NCCL_WHEEL_PLATFORM='x86_64'

# LunarG Vulkan SDK. VULKAN_LATEST_URL answers the newest Linux SDK version as
# plain text, VULKAN_FILES_JSON carries its sha256. The tarball unpacks as
# <version>/x86_64/{bin,include,lib,share}; its top directory is stripped, so
# the SDK proper (what VULKAN_SDK must name) is <Root>/<version>/linux/x86_64.
VULKAN_ROOT="$DIST_ROOT/VulkanSDK"
VULKAN_MARKER='.vulkan-version'
VULKAN_LATEST_URL='https://vulkan.lunarg.com/sdk/latest/linux.txt'
VULKAN_FILES_JSON='https://vulkan.lunarg.com/sdk/files.json'
VULKAN_TARBALL='https://sdk.lunarg.com/sdk/download/%s/linux/vulkansdk-linux-x86_64-%s.tar.xz'

# sccache, from its GitHub release (the distro package lags by many versions).
# Versionless install path on purpose: 02-build.sh passes it to CMake as the
# compiler launcher, and a moving launcher path changes every compile command.
SCCACHE_RELEASE_API='https://api.github.com/repos/mozilla/sccache/releases/latest'
SCCACHE_BIN="$HOME/.local/bin/sccache"

# Rust toolchain installer (rustup), for llama-cpp-config.
RUSTUP_INIT_URL='https://sh.rustup.rs'
