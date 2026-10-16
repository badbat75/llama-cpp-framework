# Where the external runtime/toolchain distributions come from, and where the
# BUILD machine keeps them. Sources only, NO versions: every reader resolves
# the newest STABLE release at run time and takes it, so a new ROCm, CUDA or
# Vulkan SDK lands on the next run.
# Read by 00-install-prerequisites.ps1, 01-configure.ps1 and common.ps1 (build
# machine: ROCm dist, CUDA Toolkit, Vulkan SDK). End users never see it:
# 03-package.ps1 copies the runtime libraries the backends load out of these
# dists into the installer.
@{
    # Build-machine layout: <DistRoot>\<Dir>\<Version>\<DistOs>, e.g.
    # E:\TheRock\10.1.0\windows. One directory per version, and under it one
    # per OS: the drive is shared with the Linux side of the same machine,
    # whose dist of the same version sits beside it as <Version>\linux
    # (dist-sources.sh), and neither build ever sees the other's files. Dists
    # live side by side: an upgrade lands next to its predecessor, and nothing
    # here ever deletes one (that call is the user's). Downloads go to
    # <DistRoot>\<Dir>\.downloads and are deleted after a successful install.
    # The layout holds the GPU SDKs only: OpenSSL is a system library, kept
    # where winget installs it (00-install-prerequisites.ps1).
    #
    # DistRoot is the DEFAULT, the counterpart of /mnt/data on Linux. It is
    # overridden, in this order, by -DistRoot on 00-install-prerequisites.ps1
    # or 01-configure.ps1, by the LLAMA_FW_DIST_ROOT environment variable, and
    # by the DistRoot that 01-configure.ps1 recorded in
    # build\windows\config-build.psd1 (Resolve-DistRoot in dists.ps1).
    DistRoot = 'E:\'
    DistOs   = 'windows'

    # AMD ROCm/HIP - TheRock dist tarball (the classic HIP SDK installer is
    # discontinued). Multiarch on purpose: BLAS kernels for every gfx family,
    # so one install serves whatever GPU the machine has.
    #
    # HIP_PATH names the ACTIVE dist. Dists therefore live side by side: an
    # upgrade lands next to its predecessor instead of wiping it, so going
    # back is a HIP_PATH move (plus 01-configure.ps1 + a rebuild), not a
    # 4.6 GB re-download. The cost is disk, ~25 GB each.
    #
    # The newest stable is FOUND, never composed: both scripts scan Indexes
    # (HTML pages carrying their listing as an inline JS `files` array) for
    # therock-dist-windows-multiarch-<X.Y.Z>.tar.gz, matched strictly
    # (\d+\.\d+\.\d+ right before the extension) so `-tests-` builds and dated
    # nightlies (10.1.0a20260831) are never taken as releases, and take the
    # highest. That matters because the HOST MOVED, and a stale URL fails as a
    # 403 that reads exactly like "not published yet": from ROCm 10.0 stable
    # the tarballs live at https://stable.repo.amd.com/rocm/core/tarball/ (rc:
    # rc.repo.amd.com, nightly: nightly.repo.amd.com, same /rocm/core/tarball/
    # path), while everything up to 7.14 AND the 10.0.0 release candidates
    # stay on the legacy index repo.amd.com/rocm/tarball-multi-arch/ (with
    # rocm.prereleases.amd.com for its rcs). Source: ROCm/TheRock RELEASES.md,
    # "Installing multi-arch releases". Prerelease indexes stay out.
    #
    # A dist bump costs a full rebuild (02-build.ps1 wipes the cmake dir when
    # the compiler path changes) and re-derives GpuTargets. A dist whose clang
    # is below 24 is refused by 02-build.ps1 (its HIP runtime wrapper clashes
    # with MSVC's <cmath>; clang 24 fixed it upstream).
    Rocm = @{
        Dir         = 'TheRock'            # <DistRoot>\TheRock\<Version>\<DistOs>
        Marker      = '.therock-version'   # written only after a successful extract
        Indexes = @(
            'https://stable.repo.amd.com/rocm/core/tarball/'    # 10.0 onwards
            'https://repo.amd.com/rocm/tarball-multi-arch/'     # up to 7.14
        )
    }

    # NVIDIA CUDA. RedistIndex lists one redistrib_<X.Y.Z>.json per toolkit
    # release, each naming every component's version, archive path and
    # sha256; the highest X.Y.Z is the newest toolkit.
    # The toolkit is assembled from the REDIST archives (the windows-x86_64
    # entries of that manifest, each sha256-verified) rather than NVIDIA's
    # installer, as on Linux: plain zips, any install prefix, no display
    # driver touched, nothing registered machine-wide. The Components are what
    # ggml-cuda and CMake's FindCUDAToolkit use (compiler, cudart, cuBLAS,
    # CCCL) plus the small tools around them. Each archive unpacks as
    # <name>-archive\{bin,include,lib,...}; with that level stripped they
    # merge into ONE prefix, <DistRoot>\CUDA\<X.Y.Z>\<DistOs>, which then
    # looks like a regular toolkit tree (bin\nvcc.exe, lib\x64\*.lib, nvvm\).
    # The installer carries its cuBLAS runtime (cublas64_<major> +
    # cublasLt64_<major>, in bin\x64); cudart is linked statically.
    Cuda = @{
        Dir         = 'CUDA'
        Marker      = '.cuda-version'      # written only after every component is extracted
        RedistIndex = 'https://developer.download.nvidia.com/compute/cuda/redist/'
        Platform    = 'windows-x86_64'
        Components  = @(
            'cuda_nvcc', 'cuda_crt', 'libnvvm', 'libnvptxcompiler', 'libnvfatbin',
            'libnvjitlink', 'cuda_cudart', 'libcublas', 'cccl', 'cuda_nvrtc',
            'cuda_nvtx', 'cuda_profiler_api', 'cuda_cuobjdump', 'cuda_nvdisasm',
            'cuda_cuxxfilt', 'cuda_nvprune', 'cuda_nvml_dev'
        )
    }

    # LunarG Vulkan SDK (build machine only: end users get vulkan-1.dll with
    # any GPU driver). LatestUrl answers the newest Windows SDK version as
    # plain text; the installer is a Qt installer run headless into
    # <DistRoot>\VulkanSDK\<version>\<DistOs>.
    Vulkan = @{
        Dir         = 'VulkanSDK'
        Marker      = '.vulkan-version'    # written once the installer left a complete SDK
        LatestUrl   = 'https://vulkan.lunarg.com/sdk/latest/windows.txt'
        Installer   = 'https://sdk.lunarg.com/sdk/download/{0}/windows/vulkansdk-windows-X64-{0}.exe'
        Signer      = 'LunarG, Inc.'         # Authenticode O= the installer must carry before it runs elevated
    }
}
