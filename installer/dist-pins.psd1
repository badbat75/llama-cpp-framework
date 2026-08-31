# Pinned external runtime/toolchain distributions - single source of truth.
# Read by 00-install-prerequisites.ps1 (build machine) AND by
# installer\install-runtime-deps.ps1 (bundled into the installer, runs on
# end-user machines). Bump versions HERE, deliberately, then:
#   - build machine: re-run 00-install-prerequisites.ps1 (converges to the pin)
#   - end users: get the new pins with the next installer release
@{
    # AMD ROCm/HIP - TheRock dist tarball (the classic HIP SDK installer is
    # discontinued). Multiarch on purpose: BLAS kernels for every gfx family,
    # so one install serves whatever GPU the machine has.
    #
    # One directory per version under Root (<Root>\<Version>, e.g.
    # C:\TheRock\10.0.0), and HIP_PATH names the ACTIVE one. Dists therefore
    # live side by side: an upgrade lands next to its predecessor instead of
    # wiping it, so going back is a HIP_PATH move (plus 01-configure.ps1 +
    # a rebuild), not a 4.6 GB re-download. The cost is disk, ~25 GB each:
    # 00-install-prerequisites.ps1 lists what is installed but never deletes
    # a dist, that call is the user's.
    #
    # Dists is an ordered candidate list, first REACHABLE wins, and it exists
    # for two different readers: on a machine already on a dist it is only
    # consulted when the Pin is missing (an unpublished pin then reports
    # "not published yet" and changes nothing), while on a bare machine the
    # end-user helper takes the first entry it can download. Hence the
    # previous stable stays listed BELOW the pin: it is what a fresh install
    # falls back to while the pinned version is still propagating. Prereleases
    # are deliberately NOT listed (a pinned stable that is not published yet
    # must install nothing, not an rc).
    #
    # The HOST MOVED, and a stale URL fails as a 403 that reads exactly like
    # "not published yet": from ROCm 10.0 stable (and 10.1 nightlies) the
    # tarballs live at https://stable.repo.amd.com/rocm/core/tarball/ (rc:
    # rc.repo.amd.com, nightly: nightly.repo.amd.com, same /rocm/core/tarball/
    # path), while everything up to 7.14 AND the 10.0.0 release candidates
    # stay on the legacy index repo.amd.com/rocm/tarball-multi-arch/ (with
    # rocm.prereleases.amd.com for its rcs). The file name is unchanged. So a
    # version bump means picking the index by version, not just editing the
    # number in the URL. Source: ROCm/TheRock RELEASES.md, "Installing
    # multi-arch releases".
    #
    # Indexes are scanned by 00-install-prerequisites.ps1 to REPORT a stable
    # newer than the Pin, never to install one: the pin stays the thing that
    # gets installed, on the build machine and on every end-user machine the
    # installer carries this file to. Detection cannot be the installer,
    # because a dist bump is not free: it can move the clang resource major
    # (patches\hip\<major>\ has to be regenerated or 02-build.ps1 fails fast),
    # it re-derives GpuTargets, it costs ~25 GB of disk beside the dist in
    # use, and a new ROCm can regress what the current one does right (10.0.0
    # did not fix the gfx1201 hipBLASLt bug 7.14.0 has). So this is the same
    # deal 00-install-prerequisites.ps1 offers for llama.cpp: it says a newer
    # release exists and what to do about it, the move itself stays a
    # deliberate edit here. Both hosts are listed because the index a version
    # lives on depends on the version (see above), which is also what the scan
    # removes as a failure mode: it finds the URL instead of composing it.
    # Prerelease indexes stay out for the same reason they stay out of Dists.
    Rocm = @{
        Pin     = '10.0.0'
        Root    = 'C:\TheRock'         # dists install as <Root>\<Version>; the active one is HIP_PATH
        Marker  = '.therock-version'   # written only after a successful extract
        Indexes = @(
            'https://stable.repo.amd.com/rocm/core/tarball/'    # 10.0 onwards
            'https://repo.amd.com/rocm/tarball-multi-arch/'     # up to 7.14
        )
        Dists  = @(
            @{ Version = '10.0.0'     # 4.5 GB, published 2026-08-26
               Url = 'https://stable.repo.amd.com/rocm/core/tarball/therock-dist-windows-multiarch-10.0.0.tar.gz' }
            @{ Version = '7.14.0'     # legacy index: everything before 10.0 stayed there
               Url = 'https://repo.amd.com/rocm/tarball-multi-arch/therock-dist-windows-multiarch-7.14.0.tar.gz' }
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
