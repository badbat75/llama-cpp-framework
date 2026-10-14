# llama.cpp-framework

> **Windows first**: the full pipeline (build, installer) is PowerShell (.ps1) and targets Windows 10/11. Linux has a bash counterpart that ends in a `.deb` and an `.rpm`, see [Linux](#linux).

PowerShell scripts to build and run [llama.cpp](https://github.com/ggerganov/llama.cpp) on Windows with multi-GPU backend support (CUDA, Vulkan, ROCm/HIP) and NSIS installer packaging. The chat UI built into `llama-server` is used as the frontend; no separate web UI is bundled.

## Prerequisites

- Windows 10/11
- [Visual Studio](https://visualstudio.microsoft.com/) with C++ workload
- [Git](https://git-scm.com/)

Installed/updated via winget by `00-install-prerequisites.ps1`:
- [PowerShell 7+](https://github.com/PowerShell/PowerShell)
- [OpenSSL](https://slproweb.com/products/Win32OpenSSL.html)
- [NSIS](https://nsis.sourceforge.io/) (for installer packaging)

Installed/updated from its GitHub release (winget lags upstream by days) into `%LOCALAPPDATA%\Programs\sccache`, also by `00-install-prerequisites.ps1`:
- [sccache](https://github.com/mozilla/sccache) (compiler cache for the C/C++ and HIP compiles)

GPU SDKs, installed/updated to their newest stable release by `00-install-prerequisites.ps1`, side by side as `<DistRoot>\<Name>\<Version>\windows` (e.g. `E:\TheRock\10.1.0\windows`):
- [AMD ROCm (TheRock dist)](https://rocm.docs.amd.com/) in `TheRock`
- [CUDA Toolkit](https://developer.nvidia.com/cuda-toolkit) in `CUDA` (assembled from NVIDIA's redist archives; no driver touched)
- [Vulkan SDK](https://vulkan.lunarg.com/) in `VulkanSDK`

`DistRoot` defaults to `E:\` (`installer\dist-sources.psd1`). Pass `-DistRoot <path>` to `00-install-prerequisites.ps1` and `01-configure.ps1`, or set `LLAMA_FW_DIST_ROOT`, to keep them elsewhere; `01-configure.ps1` records the root, so the later steps need nothing. The build takes its GPU SDKs from this layout only.

## Quick start

```powershell
# 1. Install / update the toolchain (elevates via UAC as needed).
#    Re-run any time to upgrade winget packages and sccache and check
#    llama.cpp for a newer release tag.
.\00-install-prerequisites.ps1

# 2. Auto-detect paths and generate config-build.psd1
.\01-configure.ps1

# Or override the llama.cpp source clone location:
.\01-configure.ps1 -LlamaCppDir "C:\path\to\my\llama.cpp"

# Or keep the SDKs on another drive (give 00 the same -DistRoot):
.\01-configure.ps1 -DistRoot "D:\sdk"

# 3. Build llama.cpp
.\02-build.ps1

# 4. Produce a Windows installer in dist\
.\03-package.ps1
```

## Linux

Debian/Ubuntu: the scripts end at a working `llama-server` with the CUDA, Vulkan and HIP backends and `llama-cpp-config` beside it, packaged as a `.deb` and an `.rpm`.

```bash
# 1. apt packages (sudo), sccache, Rust (rustup), and the newest stable ROCm/TheRock dist,
#    CUDA Toolkit and Vulkan SDK. Re-run any time to upgrade.
./00-install-prerequisites.sh

# 2. Auto-detect paths and generate build/linux/config-build.sh
./01-configure.sh

# 3. Build llama.cpp and llama-cpp-config into build/linux/llama.cpp-cmake/bin/
./02-build.sh

# 4. Package: dist/llama-cpp-framework-v<version>-<llamaRelease>-amd64.deb and -x86_64.rpm
./03-package.sh

# 5. Install (or upgrade, or reinstall) the newest package in dist/: the .deb or the .rpm,
#    as the distribution takes (sudo)
./04-install.sh
```

The packages (about 1.2 GB each, 3.3 GB installed) are self-contained under `/opt/llama.cpp/`: the binaries in `bin/` and, in `lib/`, the ROCm, CUDA and Vulkan runtime libraries the backends need, so the target machine needs the GPU drivers and nothing else. They also install a symlink per executable in `/usr/bin/` (`llama-server`, `llama-cpp-config`, the other llama.cpp tools), a desktop entry that puts the configurator ("llama.cpp Config") in the applications menu, and nothing else: `llama-cpp-config` keeps `server.ini`, `presets.ini`, `presets.ini.disabled` and `settings.ini` per user in `~/.config/llama.cpp/` (logs, benchmark runs and saved states in `~/.local/share/llama.cpp/`). A backend whose GPU or driver is missing is skipped at start.

The dists install side by side as `<root>/<version>/linux` under `/mnt/data` (`TheRock`, `CUDA`, `VulkanSDK`; for another mountpoint pass `--dist-root <path>` to `00-install-prerequisites.sh` and `01-configure.sh`, which records it, or set `LLAMA_FW_DIST_ROOT`), about 27 GB in all, and nothing is added to the environment: the scripts find the newest of each on every run. Everything generated lives under `build/linux/`, apart from the Windows artifacts under `build/windows/`. Details: [docs/build-pipeline.md](docs/build-pipeline.md#linux).

## Layout

All build artifacts live under `build\`, in a per-OS directory (`build\windows\`, `build/linux/` for the Linux scripts), so a single `rm -rf build\` returns the repo to a clean state.

```
<repo>/
├── build/
│   └── windows/
│       ├── .sccache/           ← sccache compile cache
│       ├── cargo-target/       ← cargo target dir of llama-cpp-config
│       ├── config-build.psd1   ← build-time config (generated by 01-configure.ps1)
│       ├── llama-cpp.nsi       ← NSIS script (generated by 03-package.ps1, transient)
│       ├── llama.cpp/          ← llama.cpp source clone (LlamaCppDir)
│       ├── llama.cpp-cmake/    ← cmake binary dir (out-of-source build)
│       └── staging/            ← cmake --install staging area for NSIS
├── dist/                    ← final installer .exe
├── installer/               ← NSIS template and dist-sources.psd1 (where external dists come from)
├── llama-cpp-config/        ← Rust GUI + CLI configurator (source)
├── patches/                 ← checked-in build workarounds (HIP clang runtime wrapper, per clang major)
├── resources/               ← icon generator (llama.ico is built on demand, gitignored)
└── *.ps1                    ← top-level build/package scripts
```

## Configuration

Settings are split between build-time and runtime so the same scripts work both in-tree and from a system install.

| File | Where | Purpose |
|------|-------|---------|
| `config-build.psd1` | `build\windows\` (gitignored) | Build-time paths and flags. Generated by `01-configure.ps1`. |
| `server.ini` | `%LOCALAPPDATA%\llama.cpp\config\` | Machine-wide llama-server runtime (port, host, threads, threads-batch, cache-reuse, load mode, models dir, max simultaneous models, GPU distribution via device / split-mode / tensor-split, and the image encoder's device). Single `[Server]` section. Written by `llama-cpp-config`. |
| `presets.ini` | `%LOCALAPPDATA%\llama.cpp\config\` | Per-model presets: one `[<model-id>]` section each with model path, ctx size, GPU layers, GPU distribution (device / split-mode / tensor-split), batch sizes, cache types, flash-attn, sampling, MoE expert offload, chat-template kwargs, speculative decoding (draft model / spec-type / draft device+layers). Consumed directly by `llama-server --models-preset`. Written by `llama-cpp-config`. |

> **GPU distribution** is edited as a table, not as free text: `--tensor-split` is a positional vector over the devices named by `--device`, so the configurator lists the detected GPUs with a checkbox and a weight each and derives both settings from that. The [configurator README](llama-cpp-config/README.md#gpu-distribution-table) has the details, including the two footguns it exists to make visible: a blank tensor-split over 2+ GPUs means *split by free VRAM*, not evenly; and the **image encoder (mmproj/CLIP) is placed on its own**: since llama.cpp v0.4.1 it follows the first `--device` entry, and with no device pinned (or on any older build) llama.cpp puts it on the first GPU backend it finds, where it holds VRAM but only works on image requests; the Vision encoder dropdown overrides both.

`config-build.psd1` keys:

| Section | Keys |
|---------|------|
| Paths | `DistRoot`, `LlamaCppDir`, `OpenSSLDir`, `HipPath`, `VsDevShell` |
| Build | `GpuTargets`, `BuildType`, `CCompiler`, `CxxCompiler`, `Linker`, `BuildJobs` |

Runtime configs are created and edited with `llama-cpp-config` (GUI when launched with no arguments, headless CLI via `server` / `preset` subcommands). The installer ships it in `bin\` next to `llama-server.exe`, with an optional Start Menu shortcut ("llama.cpp Config"). See [llama-cpp-config\README.md](llama-cpp-config/README.md).

## Scripts

| Script | Description |
|--------|-------------|
| `00-install-prerequisites.ps1` | Idempotent toolchain bootstrapper. Installs missing winget packages (PowerShell 7+, OpenSSL, NSIS) and upgrades present ones in a single UAC-elevated session, which also installs the newest stable ROCm/TheRock dist, CUDA Toolkit and Vulkan SDK into `<DistRoot>\<Name>\<Version>\windows`; installs or updates sccache from its latest GitHub release (sha256-verified, user scope) and removes a winget copy left by an earlier setup; fetches the llama.cpp clone and flags a rebuild when a newer release tag is available; reports version changes. Accepts `-DistRoot`. |
| `dists.ps1` | The SDK dist layout shared by the scripts: resolves `DistRoot` and finds the newest complete dist of each SDK. Not run directly. |
| `01-configure.ps1` | Detects environment, verifies tools, writes `config-build.psd1`. Accepts `-LlamaCppDir` and `-DistRoot`. |
| `02-build.ps1` | Builds **both** llama.cpp (CMake configure out-of-source into `build\windows\llama.cpp-cmake\` + Ninja, checked out at the newest `vX.Y.Z` release tag: upstream also tags every merge to master `bNNNN`, and those nightlies are not candidates) **and** `llama-cpp-config` (`cargo build --release`; no intermediate copy: packaging stages it straight from cargo's target dir). Auto-clones llama.cpp into `build\windows\llama.cpp\`. Uses sccache when available. |
| `03-package.ps1` | Stages from `build\windows\llama.cpp-cmake\` + `build\windows\cargo-target\release\` into `build\windows\staging\`, then runs NSIS to produce `dist\llama-cpp-framework-v<version>-<llamaBuild>-<arch>-setup.exe`. |
| `common.ps1` | Shared bootstrap: loads `$cfg` from `config-build.psd1`, prepends ROCm `bin\` to PATH, exposes `Enable-VsDevShell` as a function. VS Dev Shell activation is **opt-in**: only build/package scripts call it. |
| `llama-cpp-config\` | Rust GUI + CLI configurator for `server.ini` / `presets.ini`, opencode/Claude Code integrations, and llama-server start/stop. See [llama-cpp-config\README.md](llama-cpp-config/README.md). |

## Packaging

`03-package.ps1` produces a Windows installer at `dist\llama-cpp-framework-v<version>-<llamaRelease>-<arch>-setup.exe` (e.g. `llama-cpp-framework-v1.11.2-v0.2.0-x64-setup.exe`: the framework version first, the bundled llama.cpp release second). The installer:

- Installs llama.cpp binaries to `C:\Program Files\llama.cpp\` (`llama-cpp-config.exe` and `llama.ico` go into `bin\` next to `llama-server.exe`).
- Is self-contained for the GPU backends: the ROCm and CUDA runtime libraries the backends load (with rocBLAS's and hipBLASLt's kernels for the targeted GPU architectures) are copied out of the dists the build linked against, into `bin\` and `.kpack\`. The target machine needs the GPU drivers, which also bring the Vulkan loader, and the Visual C++ Redistributable; nothing is downloaded at install time.
- Optionally adds `bin\` to the system PATH.
- Optionally creates a Start Menu shortcut ("llama.cpp Config" → `llama-cpp-config.exe`).
- No install-time configuration page: all runtime configuration (server settings and per-model presets) is done post-install with `llama-cpp-config`.
- Supports upgrades (uninstalls the previous version first).
- Includes a full uninstaller in Add/Remove Programs. By default it preserves `%LOCALAPPDATA%\llama.cpp\` (configs, logs) across uninstalls; the user can opt in to a full wipe.

NSIS is installed automatically via winget if not already present.
