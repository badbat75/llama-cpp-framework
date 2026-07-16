# Pinned external runtime/toolchain distributions - single source of truth.
# Read by 00-install-prerequisites.ps1 (build machine) AND by
# installer\install-runtime-deps.ps1 (bundled into the installer, runs on
# end-user machines). Bump versions HERE, deliberately, then:
#   - build machine: re-run 00-install-prerequisites.ps1 (converges to the pin)
#   - end users: get the new pins with the next installer release
@{
    # AMD ROCm/HIP - TheRock dist tarball (the classic HIP SDK installer is
    # discontinued). Multiarch on purpose: BLAS kernels for every gfx family,
    # so one install serves whatever GPU the machine has. The prerelease URL
    # is a fallback for while a fresh stable is still propagating to
    # repo.amd.com - first reachable wins, and a fallback install self-heals
    # to the stable pin on a later run (recorded version != Pin -> reinstall).
    Rocm = @{
        Pin        = '7.14.0'
        InstallDir = 'C:\TheRock\build'   # becomes HIP_PATH (AMD's documented layout)
        Marker     = '.therock-version'   # written only after a successful extract
        Dists      = @(
            @{ Version = '7.14.0'
               Url = 'https://repo.amd.com/rocm/tarball-multi-arch/therock-dist-windows-multiarch-7.14.0.tar.gz' }
            @{ Version = '7.14.0rc1'
               Url = 'https://rocm.prereleases.amd.com/tarball-multi-arch/therock-dist-windows-multiarch-7.14.0rc1.tar.gz' }
        )
    }

    # NVIDIA cuBLAS runtime for the CUDA backend (cublas64_13 + cublasLt64_13,
    # normal imports of ggml-cuda.dll; cudart is linked statically). Official
    # per-component redistributable archive - no CUDA Toolkit needed on end
    # machines. Keep the major in sync with what the build links against
    # (cublas64_13 = any CUDA 13.x redist).
    CudaBlas = @{
        Version = '13.6.0.2'   # libcublas component version (CUDA 13.3.1 redist)
        Url     = 'https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-13.6.0.2-archive.zip'
        Sha256  = '62e9fa30560c8f0a28e0cdcf9d6fc1fed347bcfab8847239b9ae1fdc1d86408a'
    }

    # Microsoft Visual C++ Redistributable x64 - required by every shipped
    # binary (VCRUNTIME140/MSVCP140). aka.ms permalink follows the latest
    # release of the VS 18 toolset line (which the build uses).
    VcRedist = @{
        Url = 'https://aka.ms/vs/18/release/vc_redist.x64.exe'
    }
}
