---
title: Compiling llama.cpp and the configurator
type: reference
updated: 2026-11-01
---

# Compiling llama.cpp and the configurator

What `xtask configure` and `xtask build` do and why: the recorded config, the release tag and the
patches, the CMake line, the compiler cache, LTO and the linker, the GPU targets, the job count, and
what is specific to Windows and Linux. The [source map](#source-map) closes the page. The phases and
what breaks are in [Build pipeline](build-pipeline.md); macOS has
[its own page](build-macos.md) for what differs there.

## Configure phase

**`xtask configure` records what the build needs from this machine and fails if a required tool is
missing.** The config is still written, so you can fix the gaps one at a time.

| Recorded | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Dist root | `DistRoot` | `dist_root` | `dist_root` |
| Clone | `LlamaCppDir` | `llama_cpp_dir` | `llama_cpp_dir` |
| ROCm dist | `HipPath` | `hip_path` | n/a |
| GPU targets | `GpuTargets` | `gpu_targets` | n/a |
| Compiler | `CCompiler`, `CxxCompiler` | `c_compiler`, `cxx_compiler` | `c_compiler`, `cxx_compiler` |
| Linker | `Linker` | `linker_type` | n/a |
| nvcc host compiler | n/a | `cuda_host_compiler` | n/a |
| Jobs and build type | `BuildJobs`, `BuildType` (Release) | `build_jobs`, `build_type` | `build_jobs`, `build_type` |
| Windows only | `OpenSSLDir`, `VsDevShell`, `WixExe` | n/a | n/a |

**The CUDA Toolkit, NCCL and the Vulkan SDK are shown, not recorded.** The build finds the newest of
each on every run. Hand edits to the config survive until the next configure.

**The compiler is stored by full path.** On Windows, the dist's `lib\llvm\bin\clang.exe` comes
before PATH, because PATH may name another dist. A bare `clang` fails at cmake time in consoles
whose PATH predates the machine-environment change. On Linux it is the dist's `clang` and `clang++`,
then a system clang, then GCC.

**Required tools.**

- Windows: `cmake`, `ninja`, `clang`, `git`, `VsDevShell` and OpenSSL. Tools are detected in the VS
  environment, and fall back to `HipPath\bin` and `HipPath\lib\llvm\bin`.
- Linux: `git`, `cmake`, `ninja`, `g++` and `cargo`.
- macOS: Apple's `clang`, `git`, `cmake`, `ninja` and `cargo`.
- Missing `node` or `npm` on Linux and macOS is only a warning, since the chat UI then falls back to
  the prebuilt download.

**WiX is checked but never a gap.** `WixExe` is `wix.exe` on PATH, else
`%USERPROFILE%\.dotnet\tools`. A missing or off-pin version is a warning, because only
`xtask package` needs it.

## Release tag and patches

**`xtask build` checks out the newest llama.cpp release tag, never master and never a nightly.**
Upstream tags every master merge with a lightweight `bNNNN` and each release also with an annotated
`vX.Y.Z`. Nightlies are ignored even when newer.

- `git describe --abbrev=0` would return the nearest nightly, so xtask lists the `v[0-9]*` tags
  merged into `origin/master` and takes the highest by version.
- Only exact `vX.Y.Z` names count, so a `-rc` is not mistaken for a release.
- Comparison is numeric, so v0.10.0 outranks v0.2.0.
- No reachable release tag is a hard failure.
- A missing clone is cloned first.

**Patches are provisional.** `patches/llama.cpp/*.patch` apply onto the fresh tag.

- They are reverted before a checkout that moves HEAD, or when one applies in neither direction,
  because a tag bump touching a patched file would abort the checkout.
- A re-run on the same tag rewrites nothing, so nothing recompiles.
- What the reverts leave behind, such as an older patch revision or a hand edit, is stashed in the
  clone and never discarded.
- The revert and the apply both probe with `git apply --check`, so a re-run is a no-op.
- A patch that applies in neither direction fails the build instead of being skipped.
- When upstream lands the same support, the patch stops applying and is deleted, not rebased. The
  provenance and the proving log line are in `patches/llama.cpp/README.md`.

**The chat UI must end up in the binary.** `xtask build` refuses to finish without
`tools/ui/dist/index.html` under the build directory, or a `dist` in the source tree. It reports
which path produced the UI, and the download path leaves a `.ui-stamp`. See `LLAMA_BUILD_UI` below
for why this guard exists.

## CMake options

**The configure line has two halves.** `xtask/cmake/` says what is built and is machine-independent.
Each `build_llama.rs` adds what depends on this machine, and owns the `-U` unsets.

| File | Applies to | Today |
| --- | --- | --- |
| `xtask/cmake/common.toml` | Every OS: the generator (`Ninja`) and the defines | Checked in |
| `xtask/cmake/<os>.toml` | One OS: its defines follow the common ones and replace an entry of the same name | `macos.toml` only |

An entry is one `NAME[:TYPE]=VALUE` string. An invalid one or an unknown key fails before cmake
runs, so a typo fails as a typo. `cmake_options` in `xtask/src/common/cmake_options.rs` reads them,
and each entry's full reason sits beside it in the file. There is no `linux.toml`, since nothing
Linux builds is both Linux-only and machine-independent.

| Option | Why |
| --- | --- |
| `GGML_NATIVE=OFF` | The binaries ship to other machines; ISA-specific code is in the CPU variants |
| `GGML_BACKEND_DL=ON` | Backends load separately, so a machine missing a vendor's runtime skips that backend and still starts |
| `GGML_CPU_ALL_VARIANTS=ON` | One ggml-cpu library per ISA level, chosen at run time by ggml's feature score |
| `GGML_CUDA`, `GGML_VULKAN`, `GGML_HIP` `=ON` | The three GPU backends |
| `GGML_CUDA_FA_QUANTS=q4_0-q4_0,q8_0-q8_0,q5_0-q5_0,q5_1-q5_1,bf16-bf16` | Adds q5 pairs to the native FlashAttention kernels, see below |
| `GGML_LTO=ON` | See [LTO and `-march`](#lto-and--march) |
| `LLAMA_BUILD_IS_DEV=OFF` | Only release tags are checked out, so a `-dev` suffix in `--version` and the configurator's badge would be false |
| `LLAMA_BUILD_UI=ON` | Builds the UI from the same tag as the server, see below |
| `LLAMA_USE_PREBUILT_UI=ON` | If the npm build fails, fetches the prebuilt UI of this build number. `OFF` makes the guard fail the build |
| `CMAKE_C_FLAGS=-w -Wno-error=incompatible-pointer-types` | See [LTO and `-march`](#lto-and--march) for the flag; `-w` because upstream's warnings bury the output that matters |
| `CMAKE_CXX_FLAGS`, `CMAKE_CUDA_FLAGS`, `CMAKE_HIP_FLAGS` `=-w` | The same. `CMAKE_HIP_FLAGS` matters on Linux; on Windows the sources compile as CXX |

**`GGML_CUDA_FA_QUANTS` keeps q5 caches off the slow path.** Upstream's default is
`q4_0-q4_0;q8_0-q8_0;f16-f16;bf16-bf16`, with `f16-f16` added whatever the list says. A pair outside
the list still runs on the GPU. But every decode step of 1 or 2 query tokens then converts the whole
used K and V to f16, and llama-server logs "no FlashAttention vector kernel compiled ... (slow)".

- Batched steps (prefill, MTP verify batches of 3 or more) convert to f16 for every type and are
  unaffected.
- Mixed pairs, such as a q8_0 K with a q5 V, are not listed and stay slow.
- The value uses commas, because a comma cannot turn into a CMake list on the way through the
  command line.
- Each pair costs about 80 s of nvcc and 160 s of HIP clang, plus 1.6 MB of `ggml-cuda.dll` and 7 MB
  of `ggml-hip.dll`.

**`LLAMA_BUILD_UI=ON` is pinned because upstream's `OFF` default can ship no UI.** Under `OFF`, the
UI is downloaded from a HuggingFace bucket at configure time. When that fails, the build proceeds
with no UI on only a cmake warning.

**The machine-derived half.**

| What | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Compilers | `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER` | The same, with `CMAKE_HIP_COMPILER`, `CMAKE_HIP_COMPILER_ROCM_ROOT` | `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER` |
| GPU targets | `GPU_TARGETS`, `-UGPU_BUILD_TARGETS` | The same | n/a |
| Linker | `CMAKE_LINKER`, `-U` of the three linker flags | The three flags set, `-UCMAKE_LINKER_TYPE` | Nothing |
| Vulkan | Four `Vulkan_*` paths | The same | The same |
| CUDA | `CMAKE_CUDA_COMPILER` | The same, with `CUDAToolkit_ROOT`, `CMAKE_BUILD_RPATH`, `CMAKE_CUDA_HOST_COMPILER` | n/a |
| Other | `OPENSSL_ROOT_DIR` | `GGML_HIP_RCCL`, `NCCL_INCLUDE_DIR`, `NCCL_LIBRARY` | n/a |
| Launchers | C, CXX; `-U` CUDA | C, CXX, HIP; `-U` CUDA | C, CXX; `-U` OBJC |

**These are CACHE entries, so deleting a line does nothing on an existing build directory.** The
entry keeps its cached value. To retire an option, set it explicitly, or wipe
`build/<os>/llama.cpp-cmake`.

**What each change costs.**

| Change | Effect |
| --- | --- |
| Compiler path differs from the cache | The build directory is wiped: full rebuild, on every OS |
| Build directory or repo moved (Windows) | The build directory is wiped, because cmake refuses a cache that names another location |
| CUDA Toolkit version | Only the CUDA objects recompile |
| Vulkan SDK version | Only the Vulkan backend recompiles |
| `GGML_CUDA_FA_QUANTS` | ggml-cuda and ggml-hip recompile in full |
| NCCL added or removed (Linux) | ggml-cuda recompiles in full: a compile definition changes on every source |
| sccache launcher on or off (Linux) | All of ggml-hip recompiles once |
| `LLAMA_BUILD_IS_DEV` | The `llama` target recompiles, not the GPU kernels |

### Pinning the Vulkan SDK and the CUDA Toolkit

**Both are resolved from the layout and passed explicitly on every configure.** A console's stale
`VULKAN_SDK` or `CUDA_PATH`, and paths CMake has cached, would otherwise keep pointing at an older
version. `xtask build` exports each in its own process.

**Vulkan.** `FindVulkan` reuses `Vulkan_INCLUDE_DIR`, `Vulkan_LIBRARY`, `Vulkan_GLSLC_EXECUTABLE`
and `Vulkan_GLSLANG_VALIDATOR_EXECUTABLE` without checking the files exist, so all four are passed
(the validator only when present). This replaces a build directory wipe, which would cost a full
CUDA and HIP rebuild.

**CUDA.** The toolkit is chosen by version, never by `CUDA_PATH`, which names whatever NVIDIA's
installer set up last. An upgrade leaves the old toolkit working, so nothing else would notice.

A changed `CMAKE_CUDA_COMPILER` alone makes CMake delete the whole cache and top-level `CMakeFiles`.
So on a change, the build resets only the CUDA half:

- it removes `CMakeCUDACompiler.cmake`, `CMakeDetermineCompilerABI_CUDA.bin` and `CompilerIdCUDA/`;
- it passes `-U` for `CUDA_*`, `CUDAToolkit_*`, `_cmake_CUDAToolkit_*` and
  `FIND_PACKAGE_MESSAGE_DETAILS_CUDAToolkit`;
- the `GGML_CUDA_*` options survive.

The assembled toolkit holds no CUPTI, so `CUDAToolkit_CUPTI_INCLUDE_DIR` stays `NOTFOUND`. ggml
links none of it.

## sccache

**sccache caches the C, C++ and HIP compiles, and never nvcc.** `xtask build` looks for it at the
fixed install path first, then on PATH, so a console with a stale PATH still finds the right copy.
The launcher path is part of every compile command, so it must not move.

| Setting | Value | Why |
| --- | --- | --- |
| `SCCACHE_DIR` | `build/<os>/.sccache` | Per OS |
| `SCCACHE_CACHE_SIZE` | `10G` | |
| `SCCACHE_IDLE_TIMEOUT` | `0` | No idle shutdown during a long build; the build stops the daemon itself |
| `SCCACHE_MAX_FRAME_LENGTH` | `104857600` | GPU multi-arch objects are large |
| `SCCACHE_CLIENT_SIDE` | `1` | sccache 0.17+: no per-compile daemon round trip. Older versions ignore it. Stats still aggregate |
| `SCCACHE_SERVER_PORT` | `4227` | Not the default 4226, see below |

**The build runs its own daemon.** A daemon's cache directory and settings win over the client's
environment. A daemon left on the default port by another process would therefore serve the whole
build from its own cache, silently. So `xtask build` stops any leftover on port 4227 and starts its
own. It prints the stats and stops the daemon once the build ends, failed builds included.

**nvcc is intentionally not wrapped.** For the PTX-only `compute_90` gencode (ggml's default
`90-virtual`), nvcc runs `ptxas -arch=compute_90` with no `-o`. sccache treats that as cacheable,
and every `.cu` dies with `Missing "cubin" file output`.

The fix is [sccache's pending pull request that stops caching ptxas verification passes
(#2809)](https://github.com/mozilla/sccache/pull/2809). Until a release carries it, nvcc stays
uncached. Do not drop `90-virtual` to dodge it, because that changes what the shipped
`ggml-cuda.dll` covers.

**HIP is cached on Windows and Linux, by different launchers.**

- Windows: the ggml-hip sources compile as CXX, so the CXX launcher wraps them.
- Linux: `CMAKE_HIP_COMPILER_LAUNCHER`.
- macOS: C and C++ only. The Metal backend's Objective-C sources compile uncached.

sccache caches the `-x hip` compile as its HIP language, with the whole `--offload-arch` list in the
hash. On 0.18.0, a repeat compile of one ggml-cuda object was a `Cache hits (HIP)` hit, 0 s against
27 s cold (read by hand, not recorded).

**Without sccache, every launcher is cleared with `-U`**, so a cache configured with sccache does
not keep calling a missing exe.

## LTO and `-march`

**No global `-march`, and LTO only as far as `GGML_LTO` reaches.**

**A global `-march` leaks into every ggml-cpu variant.** The fallback tiers, `ggml-cpu-x64.dll` and
`ggml-cpu-sse42.dll`, would carry AVX2 code. ggml's runtime score
(`ggml-cpu/arch/x86/cpu-feats.cpp`, keyed on per-variant `GGML_AVX2`-style macros) would still pick
them on pre-AVX2 CPUs. The variants carry their own `-m` flags. Everything else builds at the
compiler's default baseline, as upstream's release binaries do.

**`-Wno-error=incompatible-pointer-types` is for that `sse42` variant.** It compiles the SSSE3
branch of `ggml-cpu/arch/x86/quants.c`. Its `_mm_prefetch` calls pass a block pointer where clang's
`xmmintrin.h` wants `const char *`, a default error on recent AMD clang. Apple clang accepts the
flag and has nothing to waive.

**`GGML_LTO=ON` sets `CMAKE_INTERPROCEDURAL_OPTIMIZATION` in `ggml/src` scope only.**

| Target | LTO |
| --- | --- |
| ggml-base, every ggml-cpu variant, ggml-vulkan, the host half of ggml-hip | `-flto=thin` |
| ggml-cuda (nvcc) | No |
| llama, common, llama-server | No |

- A global `CMAKE_INTERPROCEDURAL_OPTIMIZATION=ON` would also reach the CUDA target, so it is not
  passed.
- LTO moves codegen into the links, so links are slower. sccache still caches the compiles.
- lld-link's ThinLTO backend defaults to one thread per physical core, so there is no thread flag.
- LTO depends on lld on Linux, see [lld](#lld).

## lld

**lld links everything on Windows and Linux, by a different mechanism for each half.** To tell which
linker produced a Windows binary, look for the Rich header MSVC's `link.exe` stamps into the PE's
DOS stub. lld does not write one.

| Half | How | Blank or fallback |
| --- | --- | --- |
| Windows C++ | `lld-link.exe` next to the resolved clang, recorded as `Linker`, passed as `CMAKE_LINKER` | Blank with a bare `clang` or an MSVC toolchain: CMake keeps `link.exe` |
| Windows Rust | `linker = "rust-lld.exe"` in the generated `.cargo/config.toml` | Unconditional |
| Linux C, C++, HIP, CUDA | `-fuse-ld=lld` in the three `CMAKE_*_LINKER_FLAGS`, when `ld.lld` sits next to the clang | Flags passed blank, so none survives in the cache |

**Windows C++.** It mostly pins what CMake does anyway, so that a fallback to `link.exe` cannot
happen silently.

**Windows Rust.** The bare name is deliberate. `rust-lld` ships inside the toolchain
(`<sysroot>\lib\rustlib\<target>\bin`, not on PATH), always present and version-matched to rustc. An
external `lld-link.exe` would borrow whatever LLVM is installed.

**`rustflags = ["-Clink-arg=/force:multiple"]` is load-bearing.** Skia links ICU statically
(`skunicode_icu.lib`) while the `windows` crate imports the system one (`icuuc.dll`), so every
`uloc_*` is defined twice. `link.exe` swallows that (LNK4006), and the flag makes lld do the same.

- It bites only the test binary. The second ICU arrives through the dev-dependency
  `i-slint-backend-testing`, then `i-slint-common`, `fontique` and `windows`. Without the flag,
  `cargo test` does not link.
- The accepted cost: rustflags are not per-profile, so a real duplicate symbol in the shipped binary
  would go uncaught.
- Re-test and drop it if the two ICUs stop colliding.

**Linux keeps `GGML_LTO` alive through lld.** ThinLTO objects are bitcode, GNU ld cannot link them,
and llama.cpp then drops LTO on a failed `check_ipo_supported` without failing the build.

- GNU-style drivers choose their linker by flag, hence not `CMAKE_LINKER`.
- `CMAKE_LINKER_TYPE=LLD` is not usable: CMake 4.2 defines no linker types for the HIP language, and
  the configure dies in the HIP compiler check.
- The flag also reaches the CUDA link, through g++, which finds `ld.lld` on the build PATH.

**On Windows, the three `CMAKE_*_LINKER_FLAGS` are cleared with `-U` on every configure and none is
set.** They are language-agnostic: a `-Wl,...` also reaches the CUDA DLL, which lld-link links
directly through `cmake -E vs_link_dll` and warns `ignoring unknown argument`. They are also CACHE
entries, so an experiment would outlive itself. A per-language option, such as a ThinLTO cache
(`/lldltocache:<dir>`), needs a `$<LINK_LANGUAGE:CXX>` guard through a `CMAKE_PROJECT_INCLUDE` file.

## HIP targets

**`xtask configure` derives the targets from the installed ROCm dist, by one rule on Windows and
Linux.** They are never hand-maintained, so a dist bump only needs a re-run. The rule is
`gpu_targets` in `xtask/src/common/rocm.rs`.

1. **Start** from the architectures the dist ships BLAS kernels for: `.kpack/blas_lib_gfx*.kpack`.
   The fallback is `TensileLibrary_lazy_gfx*.dat` in `bin/rocblas/library` or `lib/rocblas/library`.
2. **Drop** `gfx9xx` (CDNA, Vega).
3. **Drop** `gfx125x`.
4. **Add** the Ryzen iGPU set `gfx1035`, `gfx1036`, `gfx1103`, `gfx1152`, `gfx1153`, even if a
   future dist drops their kernels.

| Rule | Why |
| --- | --- |
| Drop `gfx9xx` | Windows has no driver for them and gfx906's kernels are gone. On Linux, where MI cards have a driver, this is a parity choice |
| Drop `gfx125x` | Only the Linux dist ships it (`gfx1250`, the `gfx125X-dcgpu` family). llama.cpp v0.6.0's `mma.cuh` uses the gfx12 WMMA builtins, which gfx1250 lacks, so one such target fails ggml-hip |
| Keep the iGPU set | The driver's HIP runtime fails kernel-module load on every visible device when one has no code object: "device kernel image is invalid" |

**The result is 20 architectures on both systems:** RDNA1 `gfx1010-1012`, RDNA2 `gfx1030-1036`,
RDNA3 `gfx1100-1103`, RDNA3.5 `gfx1150-1153`, RDNA4 `gfx1200`, `gfx1201`. Each costs about 56 MiB of
`ggml-hip`.

- A static list of those 20 is used only when no dist or no kernel list is found.
- Prune by hand in the config only if size matters, and never the iGPU set. On Linux you can also
  add an MI card.
- ROCm's `hip-config-amd.cmake` derives `--offload-arch` from `GPU_BUILD_TARGETS`, seeded from
  `GPU_TARGETS` by a `set(... CACHE ...)` that does nothing once cached. So `xtask build` passes
  `-UGPU_BUILD_TARGETS` on every configure.

## ROCm kernel stores

**A Windows ROCm dist carries two kernel stores, and `ROCBLAS_USE_HIPBLASLT` decides which is
read.** With it off on gfx1201, everything runs out of `bin\rocblas\library`, the store that looks
legacy.

| Store | Owner | Size (ROCm 10.1.0) |
| --- | --- | --- |
| `.kpack\blas_lib_gfx*.kpack` | hipBLASLt | About 5 MiB per architecture |
| `bin\rocblas\library\` | Tensile, found relative to `rocblas.dll` | About 700 MiB |

The `.kpack` scan in [HIP targets](#hip-targets) only enumerates architectures.

**Do not delete `bin\rocblas` to trim a dist.** A dist without it stops counting for the finders. A
build or package that already names that dist breaks late: the backend loads and enumerates, then
the first matmul dies with `Could not initialize Tensile host`. About half of `bin\rocblas\library`
is `gfx908` and `gfx90a`, which are never built. That CDNA half is the safe and only worthwhile
thing to delete.

## Build parallelism

**`xtask configure` sets the job count to 3/4 of the logical cores, at most `cores - 1`, so a core
stays free during long CUDA and HIP compiles.** The clamp keeps that reservation through a later
retune. A 1-core machine gets 1 job.

- The value is `BuildJobs` or `build_jobs` in the config.
- It feeds CMake and Ninja's `-j` and `cargo build --jobs`.
- `0` means all cores.
- `-j` bounds compile processes, not CPU: there is no affinity, and nvcc and threaded lld links can
  exceed it.

**On Windows, the same number caps bare cargo commands.** `.cargo/config.toml` is generated and
gitignored, so a `cargo build` or `cargo test` from a shell or IDE obeys it too.

| Key | Effect |
| --- | --- |
| `[build] jobs` | rustc and codegen |
| `[build] target-dir` | `build/windows/cargo-target`; `CARGO_TARGET_DIR` still wins |
| `[env] RUST_TEST_THREADS` | The libtest harness. An existing value wins, so `RUST_TEST_THREADS=1 cargo test` works for serial debugging |
| `[env] RUST_MIN_STACK` | 8 MiB: the e2e tests build `AppWindow` on a test thread, and the 2 MiB default leaves under 1 MiB of headroom |

**Linux and macOS write no such file, so bare cargo there is not capped.** On a repo shared with
Windows, cargo reads the Windows file on Linux too. Its linker settings are scoped to the msvc
target and do nothing there. Its `target-dir`, jobs and test threads do apply, so a bare `cargo`
lands in `build/windows/cargo-target`. Set `CARGO_TARGET_DIR=build/linux/cargo-target` to avoid
that. The wrappers already do.

**Never reconfigure or run ninja in the build directory while a build runs there.** Ninja holds
`.ninja_deps` open, so on Windows the generate step fails with
`failed recompaction: Permission denied` after rewriting `build.ninja` and the cache. The two then
disagree until the next configure.

## Windows

**The HIP compile vars are set per process and must never be set machine-wide.** `hip_env` and
`vs_env` in `xtask/src/windows/mod.rs` set them for `xtask build` and `xtask package`.

| Var | Value | Without it |
| --- | --- | --- |
| `HIP_PATH` | `HipPath` | |
| PATH | `HipPath\bin` and `HipPath\lib\llvm\bin` (where clang lives) | |
| `HIP_DEVICE_LIB_PATH` | `lib\llvm\amdgcn\bitcode` | The device compile dies with "cannot find ROCm device library" |
| `HIP_PLATFORM` | `amd` | |
| `LLVM_PATH` | `lib\llvm` | |

The Adrenalin driver's own HIP runtime (System32's `amdhip64_7.dll` and `amd_comgr_3.dll`) reads
`LLVM_PATH` at run time. Pointing it at TheRock's LLVM breaks it:

- `hipMemGetInfo` returns "invalid argument";
- devices report 0 MiB;
- every llama-server child dies with 0xC0000005 during weight upload.

The prerequisites script removes machine-wide copies, which heals a poisoned machine.

**ROCm clang 24 is the floor.** Up to AMD clang 23, the stock `__clang_hip_runtime_wrapper.h`
included `<cmath>` before the HIP device math headers, so MSVC's `_CLANG_BUILTIN2` constexpr
overloads clashed with the device declarations. Clang 24 (TheRock ROCm 10.1.0) carries [the LLVM fix
to that wrapper (#201563)](https://github.com/llvm/llvm-project/pull/201563), so the stock header is
used and nothing is force-included.

- The major is read from the dist: `lib\llvm\lib\clang\<N>`, or `lib\clang\<N>` in a legacy layout.
- A major below 24 fails fast in `xtask build` and `tools\pcie-bench\build.ps1`.
- `CMAKE_CXX_FLAGS` and `CMAKE_HIP_FLAGS` also replace the `-include` that a cache built with an
  older patched wrapper still holds.

**The VS environment comes from `VsDevCmd.bat`.** It sits beside the `Launch-VsDevShell.ps1`
recorded as `VsDevShell`, and `xtask build` and `xtask package` copy its result into their process.
`cmake --install` needs it too.

**OpenSSL.** When `OpenSSLDir` changes, `xtask build` drops the paths FindOpenSSL cached in
`CMakeCache.txt`. CMake would otherwise reuse them without checking them. The search runs again with
no build directory wipe.

**The configurator** builds with `CARGO_TARGET_DIR=build\windows\cargo-target`, and the binary stays
in `release\`. `xtask package` stages it from there.

**PowerShell libraries.**

- `scripts\dists.ps1` is the dist layout for PowerShell: `Resolve-DistRoot`, `Find-Dist`. The
  prerequisites script and `scripts\common.ps1` dot-source it. xtask has its own finders.
- `scripts\common.ps1` is the bootstrap for `tools\pcie-bench\build.ps1`, its only consumer. It sets
  `$buildRoot`, `$cfg`, `$sources`, `$distRoot`, `$cargoTargetDir`, PATH and the same HIP vars. Its
  `Enable-VsDevShell` is opt-in.
- `xtask build`, `xtask package` and the prerequisites script do not dot-source
  `scripts\common.ps1`.

## Linux

**Linux shares the topics above and adds these pieces.** Nothing is set machine-wide: no profile
script and no `/etc/environment`. `load_build_env` in `xtask/src/linux/build_llama.rs` sets
`HIP_PATH`, `ROCM_PATH`, `HIP_DEVICE_LIB_PATH`, `HIP_PLATFORM` and `PATH` for the build process
only. `ROCM_PATH` is what ggml-hip's CMakeLists reads, and its fallbacks are `/opt/rocm` and `/usr`.

**HIP is its own CMake language here.** `enable_language(HIP)` means `CMAKE_HIP_COMPILER` (the
dist's `clang++`) and `CMAKE_HIP_COMPILER_ROCM_ROOT` are passed explicitly. The MSVC `<cmath>`
workaround does not exist here.

**RCCL is linked when the ROCm dist has it.**

- It is the collective library `--split-mode tensor` reduces through when every device of the split
  is a ROCm one, in place of ggml's internal AllReduce. The internal path stages each reduction
  through pinned host memory, and it is all Windows has.
- The option is passed here and not in `common.toml`, because the Windows dist ships no RCCL and
  `find_package(rccl REQUIRED)` would fail its configure.
- It is `ON` or `OFF` on every configure, by whether the dist has
  `lib/cmake/rccl/rccl-config.cmake`. So a cached `ON` cannot outlive a dist without the library.
- Upstream's default is `OFF` ("not universally beneficial during testing"), so it is a default to
  measure, not a guarantee.

**`GGML_CUDA_ALLREDUCE` picks the path at run time, with no rebuild.** The values are `internal`,
`nccl` and `none`. The configurator exports it from Server tab, SDK Tuning, Tensor-split AllReduce
(`server.ini` `CudaAllreduce`).

**NCCL, the CUDA counterpart, is linked when its dist is installed.** `GGML_CUDA_NCCL` is `ON` by
upstream default and only needs ggml's `FindNCCL` to find the library. So the build pins
`NCCL_INCLUDE_DIR` and `NCCL_LIBRARY` to the newest dist on every configure, and clears them with
`-U` when there is none. The configure then prints "NCCL not found", and two CUDA cards in tensor
mode take the internal AllReduce.

**nvcc's host compiler is `g++`**, through `CMAKE_CUDA_HOST_COMPILER`. nvcc supports a narrow range
of host compilers, and the ROCm clang is not one. `CUDAToolkit_ROOT` is passed with the compiler, so
FindCUDAToolkit stays inside the pinned toolkit.

**`CMAKE_BUILD_RPATH` names the toolkit's `lib` by hand.** It is an implicit link directory of nvcc,
which CMake leaves out of the computed RPATH. Without it, `libggml-cuda.so` cannot load
`libcudart.so.13` and `libcublas.so.13`, and the CUDA backend is silently absent:
`llama-server --list-devices` shows no `CUDA0`.

**The build tree runs without `LD_LIBRARY_PATH`.** The binaries carry an RPATH to the dists they
linked against. Moving or deleting a dist breaks the backend that used it. The build ends by running
`llama-server --version`.

**The configurator is built last and copied beside the server**, into
`build/linux/llama.cpp-cmake/bin/`.

- It is staged and renamed over the old file, so a running configurator keeps its file.
- `paths::llama_bin` looks in the exe's own directory first, so the copy is the one to launch. The
  exe left in `cargo-target/release` finds the same tree through its `../../llama.cpp-cmake`
  fallback.
- The cargo is the one running xtask, else PATH, else `~/.cargo/bin`.
- A missing `resources/llama.ico` is generated from the configured clone before cargo runs, on every
  OS. `build.rs`'s own bootstrap assumes the default clone.
- The file-dialog backend is a target-scoped `rfd` feature set in `Cargo.toml` (`xdg-portal`,
  `wayland`), because rfd refuses to build on Linux with none.

**`cargo test` on Linux has 6 expected failures.** Their fixtures are Windows paths (`C:\m\x.gguf`),
which `std::path` does not split on Linux. The code under test is not at fault.

## Source map

| Path | What it is |
| --- | --- |
| `Cargo.toml` | The workspace: `llama-cpp-config`, `xtask` and `tools/site`. The profiles live here because cargo reads them only at the root. |
| `xtask/src/main.rs` | The phases and their options |
| `xtask/src/common/` | Shared: `dists.rs`, `llama.rs`, `cmake_options.rs`, `configurator.rs`, `prereq.rs`, `rocm.rs`, `version.rs`, `bench_share.rs` |
| `xtask/src/<os>/` | One OS's phases: `configure.rs`, `build_llama.rs`, `package.rs`, `install.rs`, `dists.rs` (the finders) |
| `xtask/cmake/` | The checked-in llama.cpp CMake options |
| `installer/dist-sources.toml` | Dist roots and download sources for xtask, with no versions |
| `installer/dist-sources.psd1` | The same data for the Windows PowerShell scripts: a source changes in both files |
| `installer/wix-toolset.psd1` | The WiX pin |
| `scripts/00-install-prerequisites.*` | The bootstrap, the one step that is not xtask's |
| `scripts\dists.ps1`, `scripts\common.ps1` | PowerShell libraries, see [Windows](#windows) |
| `patches/llama.cpp/` | Out-of-tree patches; their README says what each changes and when to drop it |
