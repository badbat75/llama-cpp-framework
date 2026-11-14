---
title: Build pipeline
type: reference
updated: 2026-11-14
---

# Build pipeline

The pipeline turns a llama.cpp release into an installable package with the CUDA, Vulkan and ROCm
backends, the `pai-config` configurator and its command line `pai`. It runs on Windows (one MSI) and
Linux (eight `.deb` and eight `.rpm` packages: the framework's GUI, command line, web service and
core, at the framework's version, and the engine `llama.cpp` with one package per GPU runtime, at their
own versions). It also runs on macOS (Apple Silicon, four `.pkg`), where it is written but untested.

**In short.** Run the bootstrap script once, which also runs `xtask prereq` on Linux and macOS. Then
run `xtask configure`, `build`, `package` and `install` in that order. `xtask <phase>` is short for
`cargo run -p xtask -- <phase>`. The GPU SDKs come from the dist layout, in their newest stable
version, and everything generated stays under `build/<os>/`. Before changing the build, read this
page and the [topic page](#topics) of what you change. [Footguns](#footguns) lists what breaks. The
MSI itself is in [installer\AGENTS.md](../installer/AGENTS.md).

![The bootstrap script, then the xtask phases prereq, configure, build, package and install, with what each writes](assets/diagrams/build-pipeline.svg)

## Topics

| Page | What it answers |
| --- | --- |
| [Build prerequisites](build-prerequisites.md) | Where do the GPU SDKs live, which one is taken, and what do the bootstrap scripts install? |
| [Compiling](build-compiling.md) | What do `configure` and `build` do and why: release tag, patches, CMake options, sccache, LTO, lld, HIP targets, build parallelism, the web UI, the Windows and Linux specifics, the source map |
| [Packaging and install](build-packaging.md) | What goes in the MSI and in each `.deb` and `.rpm`, how do the packages depend on each other, and how does `xtask install` lay them down? |
| [Building on macOS](build-macos.md) | The whole macOS story, every phase |

## Phases

| Step | What it does | What it writes |
| --- | --- | --- |
| Bootstrap script | Installs Rust and the system packages. On Windows, also the GPU dists, sccache and WiX. | System packages; Windows dists under the dist root |
| `xtask prereq` | Linux, macOS: installs sccache and the newest GPU dists. On Windows it refuses: the script does this. | Dists under the dist root |
| `xtask configure` | Detects paths and tools, derives the GPU targets, records the dist root. | `build/<os>/config-build.*`; on Windows also `.cargo/config.toml` |
| `xtask build` | Checks out the newest llama.cpp release, applies the patches, builds llama.cpp, then the web UI in `fe/` with npm, then `pai-config`, `pai`, `pai-manager` and `pai-bff`. | `build/<os>/llama.cpp-cmake/`, `build/<os>/cargo-target/`, `fe/dist/` |
| `xtask package` | Stages the binaries and GPU runtimes, then builds the packages. | `dist/` |
| `xtask install` | Installs the newest package in `dist/`, on Linux and macOS the newest of each: first install, upgrade, reinstall or downgrade. | The machine |

| Phase | Options |
| --- | --- |
| `prereq` | `--dist-root <path>`, `--skip <steps>` |
| `configure` | `--dist-root <path>`, `--llama-cpp-dir <path>` (default `build/<os>/llama.cpp`) |
| `build` | `llama` (llama.cpp) or `framework` (the web UI and the four cargo binaries) for one product (default: both, in that order) |
| `package` | `--keep-staging`; Linux `llama` or `framework` for one product's packages (default: both; the other product's files in `dist/` are left as they are), `--deb`, `--rpm`; Windows `--wix <wix.exe>`. Windows and macOS refuse a product: one MSI, one `.pkg` set |
| `install` | `--dry-run`, a package file (Linux and macOS: any number, and only those install); Linux `--deb`, `--rpm`; Linux and macOS `--force` |

**After a change, re-run the phases below.** The bootstrap report names the same steps.

| What changed | Run |
| --- | --- |
| The ROCm dist or the dist root | `configure`, `build` (a full rebuild, since the compiler path moves), `package` |
| The CUDA Toolkit, NCCL or the Vulkan SDK | `build`, `package`: the build finds the newest on every run |
| A newer llama.cpp release | `build`, `package` |
| The framework only (`crates/`, `fe/`), on Linux | `build framework`, `package framework` |
| llama.cpp's patches or CMake options only, on Linux | `build llama`, `package llama` |
| Tools or their paths | `configure` |

## Running the phases

**Windows.** Start in any PowerShell, since the script runs under Windows PowerShell 5.1. Open a new
terminal after it, because it changes the machine environment.

```powershell
.\scripts\00-install-prerequisites.ps1    # -DistRoot <path> for another root
cargo run -p xtask -- configure           # --dist-root <path> only if the script's root is not recorded
cargo run -p xtask -- build
cargo run -p xtask -- package
cargo run -p xtask -- install
```

**Linux and macOS.** The script installs the system packages and Rust, then runs `xtask prereq`
itself.

```sh
scripts/00-install-prerequisites.sh       # --dist-root <path>, --skip <steps>
cargo run -p xtask -- configure
cargo run -p xtask -- build
cargo run -p xtask -- package
cargo run -p xtask -- install
```

**The wrappers are optional.** `scripts/01-configure`, `02-build`, `03-package` and `04-install`
exist as `.ps1` (PowerShell 7) and `.sh`. They only run the cargo phase with `CARGO_TARGET_DIR` set
to `build/<os>/cargo-target`. They do not change directory, so relative arguments keep working.

A bare `cargo run -p xtask` builds xtask into `target/`, or where the generated `.cargo/config.toml`
says. The configurator always goes to `build/<os>/cargo-target`.

## How the operating systems differ

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Status | Working | Working; the script's packages are Debian and Ubuntu | Apple Silicon only, untested |
| Backends | CUDA, Vulkan, HIP | The same, with RCCL and NCCL | Metal, Vulkan through MoltenVK |
| Compiler | ROCm dist's clang | ROCm dist's clang, else system clang, else GCC | Apple's clang |
| Linker | `lld-link` | `ld.lld` | Toolchain default |
| HIP sources | Compiled as CXX | CMake's HIP language | No HIP |
| HTTPS | OpenSSL from winget, DLLs bundled | Distro OpenSSL | Static BoringSSL |
| Config file | `config-build.psd1` | `config-build.toml` | `config-build.toml` |
| `.cargo/config.toml` | Generated | Not written | Not written |
| Default dist root | `E:\` | `/mnt/data` | `~/dists` |
| Dists | `TheRock`, `CUDA`, `VulkanSDK` | The same, with `NCCL` | `VulkanSDK` |
| Machine environment | `HIP_PATH` and PATH of the active ROCm dist | None | None |
| sccache binary | `%LOCALAPPDATA%\Programs\sccache\sccache.exe` | `~/.local/bin/sccache` | `~/.local/bin/sccache` |
| Packages | One MSI | Eight `.deb`, eight `.rpm` | Four `.pkg` |
| Installs to | `Program Files\private-ai` (choosable) | `/usr` (`/usr/bin`, `/usr/lib/private-ai`, `/usr/libexec/private-ai`, `/usr/share/private-ai`) | `/opt/private-ai`, links in `/usr/local/bin` |

## Where things land

**Everything an OS generates stays under `build/<os>/`, apart from the web UI's files in `fe/`, and
the whole `build/` is gitignored.** A clone shared between systems would mix line endings, patch state and the UI's platform-specific
`node_modules`. `rm -rf build` removes the cache and the release build too.

| Path | What it holds |
| --- | --- |
| `build/<os>/config-build.psd1` or `.toml` | The machine-derived half of the build, written by `xtask configure` |
| `build/<os>/llama.cpp/` | The llama.cpp clone, detached on a release tag |
| `build/<os>/llama.cpp-cmake/` | The CMake build directory; the binaries are in `bin/` |
| `build/<os>/cargo-target/` | The cargo output of the configurator, `pai-manager` and `pai-bff` |
| `build/<os>/.sccache/` | The compiler cache |
| `build/<os>/staging/` | The package tree; deleted after packaging unless `--keep-staging` |
| `build/<os>/package/` | Linux, macOS: one tree per package, and on Linux the control files, specs and rpmbuild tree; transient |
| `build/windows/wix-obj/` | WiX intermediates; deleted after packaging |
| `build/windows/msi-install.log`, `msi-uninstall.log` | The logs of `xtask install` |
| `dist/` | The finished packages (gitignored) |
| `fe/node_modules/`, `fe/dist/` | The web UI's npm packages and its build, which `pai-bff` embeds: not per OS, since every `xtask build` remakes both (`npm ci`, `npm run build`); gitignored |
| `.cargo/config.toml` | Windows only: generated and gitignored, at the repo root because cargo discovers it by walking up from the current directory |

On Windows, `$buildRoot` in `scripts\common.ps1` names `build\windows`.

**Linux and macOS read only `config-build.toml`.** A `config-build.sh` beside it is ignored.

## Footguns

| Do not | Because |
| --- | --- |
| Set `LLVM_PATH` or the other HIP compile vars machine-wide (Windows) | The driver's HIP runtime breaks: devices report 0 MiB, llama-server dies with 0xC0000005. [Windows](build-compiling.md#windows) |
| Reconfigure or run ninja in a build directory while it builds | `build.ninja` and the cache end up disagreeing until the next configure. [Build parallelism](build-compiling.md#build-parallelism) |
| Delete a line from `xtask/cmake/` to restore a default | CACHE entries keep their value. Set it explicitly or wipe the directory. [CMake options](build-compiling.md#cmake-options) |
| Pass a global `-march` | It leaks AVX2 code into the fallback CPU variants, which ggml still picks on pre-AVX2 CPUs. [LTO and `-march`](build-compiling.md#lto-and--march) |
| Pass a global `CMAKE_INTERPROCEDURAL_OPTIMIZATION` | It reaches the CUDA target. [LTO and `-march`](build-compiling.md#lto-and--march) |
| Wrap nvcc in sccache, or drop `90-virtual` to allow it | Every `.cu` fails, or the shipped `ggml-cuda.dll` covers less. [sccache](build-compiling.md#sccache) |
| Set `CMAKE_*_LINKER_FLAGS` on Windows | They reach the CUDA link and outlive the experiment in the cache. [lld](build-compiling.md#lld) |
| Use `CMAKE_LINKER_TYPE=LLD` on Linux | The configure dies in the HIP compiler check. [lld](build-compiling.md#lld) |
| Link Linux clang objects without lld | llama.cpp drops LTO silently. [lld](build-compiling.md#lld) |
| Remove `/force:multiple` from the generated cargo config | `cargo test` stops linking on Windows. [lld](build-compiling.md#lld) |
| Prune the iGPU targets | A visible iGPU breaks the dGPU: "device kernel image is invalid". [HIP targets](build-compiling.md#hip-targets) |
| Delete `bin\rocblas` from a ROCm dist | The first matmul dies with `Could not initialize Tensile host`. [ROCm kernel stores](build-compiling.md#rocm-kernel-stores) |
| Leave two ROCm directories on PATH (Windows) | `amdhip64_*.dll` gets an ambiguous load order. [Windows prerequisites](build-prerequisites.md#windows-prerequisites) |
| Move or delete a dist a Linux build tree linked | The backend that used it stops loading. [Linux](build-compiling.md#linux) |
| Export `HIP_PATH` or `ROCM_PATH` before `xtask configure` on Linux | A usable one wins over the layout. [Dist layout and root](build-prerequisites.md#dist-layout-and-root) |
| Run bare `cargo` on Linux with the Windows `.cargo/config.toml` present | It lands in `build/windows/cargo-target`. [Build parallelism](build-compiling.md#build-parallelism) |
| Share a clone or build directory between systems | Line endings, patch state and `node_modules` mix. [Where things land](#where-things-land) |
| Test a tree-built `llama-server` on Linux while the `llama.cpp` package is installed | It loads the installed backends from `GGML_BACKEND_DIR`, `/usr/lib/private-ai`, before its own. [The `.deb` and `.rpm`](build-packaging.md#the-deb-and-rpm-linux) |
| Upgrade the Linux packages under a llama-server started by hand | It would load new kernels with old libraries; `xtask install` refuses (the services' instances are stopped and started again by `llama.cpp`'s scripts). [Install](build-packaging.md#install) |
| Leave out the runtime package of a GPU the machine has | llama-server skips that backend at start, with only a log line. [The `.deb` and `.rpm`](build-packaging.md#the-deb-and-rpm-linux) |
| Copy all of `OpenSSLDir\bin` into the package | Most of it is providers that must not ship. [The MSI](build-packaging.md#the-msi-windows) |
| Switch the MSI to external cabinets | The release is one file, under GitHub's 2 GiB limit. [installer\AGENTS.md](../installer/AGENTS.md) |
