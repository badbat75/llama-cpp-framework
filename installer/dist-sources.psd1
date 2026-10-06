# Where the external runtime/toolchain distributions come from. Sources only,
# NO versions: every reader resolves the newest STABLE release at run time and
# takes it, so a new ROCm, CUDA or Vulkan SDK lands on the next run.
# Read by 00-install-prerequisites.ps1 (build machine: ROCm dist, CUDA
# Toolkit, Vulkan SDK) AND by installer\install-runtime-deps.ps1 (bundled
# into the installer, runs on end-user machines: ROCm dist, cuBLAS redist,
# VC++ redist).
@{
    # AMD ROCm/HIP - TheRock dist tarball (the classic HIP SDK installer is
    # discontinued). Multiarch on purpose: BLAS kernels for every gfx family,
    # so one install serves whatever GPU the machine has.
    #
    # One directory per version under Root (<Root>\<Version>, e.g.
    # C:\TheRock\10.1.0), and HIP_PATH names the ACTIVE one. Dists therefore
    # live side by side: an upgrade lands next to its predecessor instead of
    # wiping it, so going back is a HIP_PATH move (plus 01-configure.ps1 +
    # a rebuild), not a 4.6 GB re-download. The cost is disk, ~25 GB each:
    # nothing here ever deletes a dist, that call is the user's.
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
    # the compiler path changes) and re-derives GpuTargets; on a clang major
    # below 24 it also needs patches\hip\<major>\ (clang 24 fixed the header
    # clash upstream).
    Rocm = @{
        Root    = 'C:\TheRock'         # dists install as <Root>\<Version>; the active one is HIP_PATH
        Marker  = '.therock-version'   # written only after a successful extract
        Indexes = @(
            'https://stable.repo.amd.com/rocm/core/tarball/'    # 10.0 onwards
            'https://repo.amd.com/rocm/tarball-multi-arch/'     # up to 7.14
        )
    }

    # NVIDIA CUDA. RedistIndex lists one redistrib_<X.Y.Z>.json per toolkit
    # release, each naming every component's version, archive path and
    # sha256; the highest X.Y.Z is the newest toolkit.
    #   - Build machine: the toolkit itself, from the NETWORK installer
    #     (cuda_<X.Y.Z>_windows_x86_64_network.exe, older releases
    #     cuda_<X.Y.Z>_windows_network.exe) run silently with an EXPLICIT
    #     subpackage list. A bare `-s` installs every subpackage, the display
    #     driver (Display.Driver) included, and the driver is the user's to
    #     manage. Subpackages get `_<X.Y>` appended; the list is what ggml-cuda
    #     and CMake's FindCUDAToolkit use (compiler, cudart, cuBLAS, CCCL)
    #     plus the small tools around them.
    #   - End users: the cuBLAS runtime only (cublas64_<major> +
    #     cublasLt64_<major>, ~400 MB), the libcublas archive of the newest
    #     release whose cuBLAS major matches the one ggml-cuda.dll imports.
    #     cudart is linked statically, so nothing else is needed.
    Cuda = @{
        RedistIndex = 'https://developer.download.nvidia.com/compute/cuda/redist/'
        Installers  = @(
            'https://developer.download.nvidia.com/compute/cuda/{0}/network_installers/cuda_{0}_windows_x86_64_network.exe'
            'https://developer.download.nvidia.com/compute/cuda/{0}/network_installers/cuda_{0}_windows_network.exe'
        )
        Root        = 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA'   # toolkits install as <Root>\v<X.Y>
        Subpackages = @(
            'nvcc', 'crt', 'nvvm', 'nvptxcompiler', 'nvfatbin', 'nvjitlink',
            'cudart', 'cublas', 'cublas_dev', 'thrust', 'nvrtc', 'nvrtc_dev',
            'nvtx', 'cuda_profiler_api', 'cuobjdump', 'nvdisasm', 'cuxxfilt',
            'nvprune', 'nvml_dev', 'visual_studio_integration'
        )
    }

    # LunarG Vulkan SDK (build machine only: end users get vulkan-1.dll with
    # any GPU driver). LatestUrl answers the newest Windows SDK version as
    # plain text; the installer is a Qt installer run headless into
    # <Root>\<version> (02-build.ps1 takes the newest complete SDK, whether
    # or not the old one survives the upgrade).
    Vulkan = @{
        LatestUrl   = 'https://vulkan.lunarg.com/sdk/latest/windows.txt'
        Installer   = 'https://sdk.lunarg.com/sdk/download/{0}/windows/vulkansdk-windows-X64-{0}.exe'
        Root        = 'C:\VulkanSDK'
    }

    # Microsoft Visual C++ Redistributable x64 - required by every shipped
    # binary (VCRUNTIME140/MSVCP140). aka.ms permalink follows the latest
    # release of the VS 18 toolset line (which the build uses).
    VcRedist = @{
        Url = 'https://aka.ms/vs/18/release/vc_redist.x64.exe'
    }
}
