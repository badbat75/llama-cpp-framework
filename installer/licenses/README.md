# Licence texts of bundled libraries

The licences of libraries a package bundles whose text the dist does not carry. `xtask\src\common\licenses.rs` says which library each covers, and stages them into `share\licenses\` of every package. The four web UI texts at the end of the table are the exception: they go only into the package that ships the web UI (the bff's on Linux, the framework's MSI on Windows, no macOS package), and `README.txt` names them only there. The texts are verbatim unless the table says otherwise, taken on 2026-11-02 from the default branch of each project; when one is refreshed, this table says from where. The four texts of the web UI at the end of the table were taken on 2026-11-18: the fonts and the icons are the `LICENSE` of the npm package and version named, `react.txt` is the `LICENSE` of the react version `fe\package.json` pins, byte-identical to react-dom's.

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
| `rocm-origami.txt` | `origami` | `ROCm/rocm-libraries`, `shared/origami/LICENSE.md` |
| `rocm-tensilelite.txt` | `tensilelite-host` (`libhipblaslt.so.1`'s host library) | `ROCm/rocm-libraries`, `projects/hipblaslt/tensilelite/LICENSE.md` |
| `nccl.txt` | NCCL | `NVIDIA/nccl`, `LICENSE.txt` |
| `vulkan-loader.txt` | Vulkan loader: `vulkan-1.dll` on Windows, `libvulkan.so.1` on Linux | `KhronosGroup/Vulkan-Loader`, `LICENSE.txt` |
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
| `atkinson-hyperlegible-next.txt` | Atkinson Hyperlegible Next, the web UI's sans font | npm `@fontsource/atkinson-hyperlegible-next` 5.3.0, `package/LICENSE` |
| `atkinson-hyperlegible-mono.txt` | Atkinson Hyperlegible Mono, the web UI's mono font | npm `@fontsource/atkinson-hyperlegible-mono` 5.3.0, `package/LICENSE` |
| `lucide.txt` | Lucide, the web UI's icons | npm `lucide-static` 1.74.0, `package/LICENSE` |
| `react.txt` | React, the web UI's library | `fe\node_modules\react\LICENSE`, react 19.3.1 as pinned in `fe\package.json` |
