# Licence texts of bundled libraries

The licences of libraries a package bundles whose text the dist does not carry. `xtask\src\common\licenses.rs` says which library each covers, and stages them into `share\licenses\` of every package. The texts are verbatim unless the table says otherwise, taken on 2026-11-02 from the default branch of each project; when one is refreshed, this table says from where.

| File | Project | Taken from |
| --- | --- | --- |
| `rocm-clr.txt` | HIP runtime (`amdhip64`) | `ROCm/rocm-systems`, `projects/clr/LICENSE.md` |
| `rocm-kpack.txt` | `rocm_kpack` | `ROCm/rocm-kpack`, `LICENSE` |
| `rocm-rocr-runtime.txt` | `hsa-runtime64` | `ROCm/rocm-systems`, `projects/rocr-runtime` |
| `rocm-amdsmi.txt` | `amd_smi` | `ROCm/rocm-systems`, `projects/amdsmi` |
| `rocm-rocprofiler-register.txt` | `rocprofiler-register` | `ROCm/rocm-systems`, `projects/rocprofiler-register` |
| `rocm-roctracer.txt` | `roctx64` | `ROCm/rocm-systems`, `projects/roctracer` |
| `rocm-rccl.txt` | `rccl` | `ROCm/rocm-systems`, `projects/rccl` |
| `rocm-rocroller.txt` | `rocroller` | `ROCm/rocm-libraries`, `shared/rocroller/LICENSE.md` |
| `nccl.txt` | NCCL | `NVIDIA/nccl`, `LICENSE.txt` |
| `vulkan-loader.txt` | Vulkan loader on Linux | `KhronosGroup/Vulkan-Loader`, `LICENSE.txt` |
| `moltenvk.txt` | MoltenVK | `KhronosGroup/MoltenVK`, `LICENSE` |
| `llvm.txt` | LLVM, clang-cpp, OpenMP | `llvm/llvm-project`, `llvm/LICENSE.TXT` |
| `bzip2.txt` | bzip2 | `gitlab.com/bzip2/bzip2`, `COPYING` |
| `libdrm.txt` | libdrm | the notice in `xf86drm.c` of `gitlab.freedesktop.org/mesa/drm`, read through the mirror `grate-driver/libdrm`: the project has no licence file |
| `elfutils.txt` | elfutils' libelf | `sourceware.org/git/elfutils.git`, `COPYING-LGPLV3` and `COPYING-GPLV2`, joined |
| `xz.txt` | liblzma | `tukaani-project/xz`, `COPYING` and `COPYING.0BSD`, joined |
| `libmnl.txt` | libmnl | the standard LGPL v2.1 text (`numactl.txt`), with a line naming the project: `git.netfilter.org` refuses scripted downloads |
| `libnl.txt` | libnl | `thom311/libnl`, `COPYING` |
| `numactl.txt` | libnuma | `numactl/numactl`, `LICENSE.LGPL2.1` |
| `zlib.txt` | zlib | `madler/zlib`, `LICENSE` |
| `zstd.txt` | zstd | `facebook/zstd`, `LICENSE` |
