# Private AI Framework

> **Windows first**: the full pipeline (build, installer) targets Windows 10/11 and ends in an MSI. Linux ends in six `.deb` and six `.rpm` packages (see [Linux](#linux)), macOS on Apple Silicon in four `.pkg` (see [macOS](#macos), untested).

A build orchestrator, run with cargo (`cargo run -p xtask -- <phase>`, the crate in `xtask/`), that builds and packages [llama.cpp](https://github.com/ggerganov/llama.cpp) with multi-GPU backend support (CUDA, Vulkan, ROCm/HIP on Windows and Linux; Metal and Vulkan on macOS) and MSI packaging (WiX Toolset) on Windows. The chat UI built into `llama-server` is used as the frontend; no separate web UI is bundled.

Beside llama.cpp it ships **`pai-config`**, a GUI configurator (Rust, [Slint](https://slint.dev); its command line is **`pai`**) that writes `llama-server`'s `server.ini` and per-model `presets.ini`, starts and stops the server, and benchmarks presets into a public SQLite database (`benchmarks/bench.db`) with a versioned method, so numbers from two machines compare like with like.

**Read the guides at [badbat75.github.io/llama-cpp-framework](https://badbat75.github.io/llama-cpp-framework/)**: the measured per-card and per-model settings with interactive charts drawn from `bench.db`, linked citations and a page per cited benchmark campaign. The same guides are the Markdown files in [docs/](docs/README.md).

## Screenshots

| | |
|---|---|
| ![Server tab](docs/assets/screenshots/server.png) | ![Models tab](docs/assets/screenshots/models.png) |
| **Server**: machine-wide `server.ini` (network, threads, models folder, conversation snapshots, GPU SDK tuning). | **Models**: one preset per model, with the GGUF header read in place (layers, attention, MTP head, vision encoder) and the GPU distribution as a table of detected devices. |
| ![Benchmark tab, standard spec](docs/assets/screenshots/benchmark-spec.png) | ![Benchmark tab, A/B](docs/assets/screenshots/benchmark-ab.png) |
| **Benchmark** with the `standard` spec chosen: the spec ticks its pinned prompts and fixes the workload fields. | **Benchmark** A/B: one preset key (`spec-draft-n-max`) at three values, each one written to the preset with a restart, put back afterwards. |

<p align="center"><img src="docs/assets/screenshots/hw-monitor.png" alt="HW Monitor" width="480"><br><b>HW Monitor</b>: VRAM, GTT spill, GPU, CPU and memory per device, once a second.</p>

## Features

**Build and packaging** ([docs/build-pipeline.md](docs/build-pipeline.md))

- One cargo orchestrator for every OS: `cargo run -p xtask -- prereq | configure | build | package | install`; the only non-cargo step is the bootstrap that installs Rust and the system packages.
- Always the newest llama.cpp **release** tag (`vX.Y.Z`, never a `bNNNN` nightly), CMake + Ninja, sccache, the machine-independent CMake options in `xtask/cmake/*.toml` with a reason for each.
- Backends: CUDA, Vulkan and ROCm/HIP on Windows and Linux (RCCL/NCCL for `--split-mode tensor` on Linux); Metal and Vulkan through MoltenVK on Apple Silicon.
- GPU SDKs (ROCm TheRock, CUDA, Vulkan SDK) installed side by side as `<root>/<Name>/<Version>/<os>`, newest stable, the layout winning over the environment; no machine-wide SDK variables.
- Self-contained packages: an MSI (WiX, upgrades, downgrade refused, ICE-validated), split `.deb` and `.rpm` packages under `/usr`, split `.pkg` on macOS; the ROCm, CUDA and Vulkan runtimes the backends load are packaged too, so a target machine needs only the GPU driver.
- Out-of-tree llama.cpp patches in `patches/llama.cpp/`, applied on every build and reverted before the next checkout, each with its source, digest and the condition to drop it: a trimmed MTP draft vocabulary for Qwen3.5/3.8, and the fix of the MTP draft graph's reallocations after an image.

**Configurator** ([crates/config/README.md](crates/config/README.md))

- Five tabs (Server, Models, Benchmark, Integrations, Settings), a tray icon, a live log viewer, and the HW Monitor window (GPU, VRAM and its spill into system memory, PCIe traffic on NVIDIA, CPU, memory, disk).
- Presets edited as forms with every llama-server knob explained in place; GPU distribution (`device`, `split-mode`, `tensor-split`) as a table of detected GPUs, tensor placement (`override-tensor`) as ordered rules, the vision encoder's GPU on its own.
- Hugging Face search and download that creates the preset and recommends a quantization that fits this machine's GPUs.
- Conversation snapshots: each model's KV cache saved on stop and restored on the next start, so a restart does not re-read a long conversation.
- Integrations: the OpenCode provider (one model per enabled preset) and a Claude Code environment snippet; log rotation; start at login.
- The same operations headless: `pai server|preset|control|bench|config ...`.

**Benchmarks** ([benchmarks/README.md](benchmarks/README.md))

- Two engines: **live** (real `llama-server` requests, cold prefill every repetition, the preset's drafter and sampler included) and **synthetic** (`llama-bench`, raw prefill and decode).
- Concurrency scenarios at the preset's `parallel`: single, N prefills at once, N decodes at once, prefill and decode mixed.
- **Versioned specs** (`benchmarks/specs/standard-<X.Y>.ini`, every version installed beside the binaries and runnable; only their digests are compiled in): prompts pinned by sha256 (43k and 115k tokens of prose, and a coding-agent turn with tool definitions and tool results), temperature, repetitions, concurrency and per-pass overrides; every run records the spec, its version and digest, and anything changed from it is recorded as a derivation, never as the spec.
- A/B of one preset key (`bench run --key ... --values ...`, or the tab's A/B field) with the key put back afterwards; an interrupted run leaves a marker with the value to restore.
- Every finished run in SQLite with the machine, the software stack (llama.cpp, ROCm, CUDA, Vulkan SDK), the model and every parameter; `bench query` to read it; absolute paths and secrets dropped on the way in, so the database can be public.
- A GPU lock every benchmark holds, plus a check that no other process has the GPUs open.

**Measured guides** ([site](https://badbat75.github.io/llama-cpp-framework/), [Markdown](docs/README.md)): the per-card settings for the Radeon AI PRO R9700 and the RTX 4070 SUPER, and per-model guides for Qwen3.8-Flash-Next and Qwen3.8-27B, each value citing the bench.db campaign it comes from.

## Prerequisites

- Windows 10/11
- [Visual Studio](https://visualstudio.microsoft.com/) with C++ workload
- [Git](https://git-scm.com/)

Installed/updated via winget by `scripts\00-install-prerequisites.ps1`:
- [PowerShell 7+](https://github.com/PowerShell/PowerShell)
- [OpenSSL](https://slproweb.com/products/Win32OpenSSL.html)
- [.NET SDK 10](https://dotnet.microsoft.com/) (hosts the WiX Toolset)

Installed/updated from its GitHub release (winget lags upstream by days) into `%LOCALAPPDATA%\Programs\sccache`, also by `scripts\00-install-prerequisites.ps1`:
- [sccache](https://github.com/mozilla/sccache) (compiler cache for the C/C++ and HIP compiles)

Installed as a .NET global tool at the version pinned in `installer\wix-toolset.psd1`, with its UI and Util extensions, also by `scripts\00-install-prerequisites.ps1`:
- [WiX Toolset](https://wixtoolset.org/) 7 (builds the MSI). WiX 7 asks each user to accept its EULA once before it builds anything (Open Source Maintenance Fee: organizations above USD 10,000 yearly revenue owe the fee); the scripts never accept it for you: read it, then run `wix eula accept wix7`.

GPU SDKs, installed/updated to their newest stable release by `scripts\00-install-prerequisites.ps1`, side by side as `<DistRoot>\<Name>\<Version>\windows` (e.g. `E:\TheRock\10.1.0\windows`):
- [AMD ROCm (TheRock dist)](https://rocm.docs.amd.com/) in `TheRock`
- [CUDA Toolkit](https://developer.nvidia.com/cuda-toolkit) in `CUDA` (assembled from NVIDIA's redist archives; no driver touched)
- [Vulkan SDK](https://vulkan.lunarg.com/) in `VulkanSDK`

`DistRoot` defaults to `E:\` (`installer\dist-sources.psd1`). Pass `-DistRoot <path>` to `scripts\00-install-prerequisites.ps1` and `--dist-root <path>` to `xtask configure`, or set `PRIVATE_AI_DIST_ROOT`, to keep them elsewhere; `xtask configure` records the root, so the later steps need nothing. The build takes its GPU SDKs from this layout only.

## Quick start

Every step is a cargo command from the repo root, except the first: the toolchain, Rust included, has to exist before cargo does.

```powershell
# 1. Install / update the toolchain (elevates via UAC as needed): winget packages,
#    Rust, sccache, WiX, the GPU SDKs. Re-run any time to upgrade them and check
#    llama.cpp for a newer release tag.
.\scripts\00-install-prerequisites.ps1

# 2. Auto-detect paths and generate config-build.psd1
cargo run -p xtask -- configure

# Or override the llama.cpp source clone location:
cargo run -p xtask -- configure --llama-cpp-dir "C:\path\to\my\llama.cpp"

# Or keep the SDKs on another drive (give the prerequisites script the same root as -DistRoot):
cargo run -p xtask -- configure --dist-root "D:\sdk"

# 3. Build llama.cpp
cargo run -p xtask -- build

# 4. Produce the Windows installer (.msi) in dist\
cargo run -p xtask -- package

# 5. Install it on this machine (UAC prompt; upgrade, repackage or downgrade alike)
cargo run -p xtask -- install
```

## Linux

Debian/Ubuntu: the same cargo phases end at a working `llama-server` with the CUDA, Vulkan and HIP backends and `pai-config` and `pai` beside it, packaged as six `.deb` and six `.rpm` packages.

```bash
# 1. First time only (no Rust yet): apt packages (sudo), Rust (rustup), then the dists.
scripts/00-install-prerequisites.sh

#    Afterwards, to update sccache and the newest stable ROCm/TheRock dist,
#    CUDA Toolkit, NCCL and Vulkan SDK:
cargo run -p xtask -- prereq

# 2. Auto-detect paths and generate build/linux/config-build.toml
cargo run -p xtask -- configure

# 3. Build llama.cpp, pai-config and pai into build/linux/llama.cpp-cmake/bin/
cargo run -p xtask -- build

# 4. Package: dist/private-ai-framework-{config,cli,llama}-v<version>-<llamaRelease>-amd64.deb,
#    dist/private-ai-framework-{rocm,cuda,vulkan}-v<sdkVersion>-amd64.deb, and the same as -x86_64.rpm
cargo run -p xtask -- package

# 5. Install (or upgrade, or reinstall) the newest package of each component in dist/: the .deb
#    or the .rpm, as the distribution takes (sudo). Name the files to install only those.
cargo run -p xtask -- install
```

The packages (about 1.7 GiB in all, 3.8 GiB installed) are self-contained, so the target machine needs the GPU drivers and nothing else:

| Package | Holds | Version |
|---|---|---|
| `private-ai-framework-config` | `pai-config`, the GUI, and its data (requires `cli` and `llama` at the same version) | framework + llama.cpp release |
| `private-ai-framework-cli` | `pai`, the command line, the benchmark method and the CLI's text (requires `llama` at the same version; no graphics library) | framework + llama.cpp release |
| `private-ai-framework-llama` | llama.cpp's programs and libraries, the ggml backends included | framework + llama.cpp release |
| `private-ai-framework-rocm` | The ROCm runtime libraries (`lib/`) | ROCm SDK |
| `private-ai-framework-cuda` | The CUDA and NCCL runtime libraries (`lib/`) | CUDA + NCCL SDK |
| `private-ai-framework-vulkan` | The Vulkan loader (`lib/`) | Vulkan SDK |

`llama` recommends the three runtimes and refuses any other version of them; leave out the ones the machine has no use for. A headless server installs `llama`, `cli` and its runtimes, without `config` and the graphics libraries it pulls in. Each package but `cli`, which is new, upgrades in place the packages of release 2.4 and earlier, `llama-cpp-framework` and `llama-cpp-framework-<component>`, which it replaces. They install the executables in `/usr/bin/` (`llama-server`, `pai-config`, `pai`, the other llama.cpp tools), the libraries and runtimes in `/usr/lib/private-ai/`, a desktop entry that puts the configurator ("Private AI Framework") in the applications menu, and, in `llama`, the `private-ai-manager` service that runs llama-server as the systemd instance `llama.cpp@default` (enabled and started on install, logging to the journal: `journalctl -u llama.cpp@default`). The configurator is its client: the server and preset files are the service's, in `/etc/private-ai/`, changed through it with polkit asking for an administrator's password; `settings.ini` stays per user in `~/.config/private-ai/` (benchmark runs in `~/.local/share/private-ai/`). At its first start `pai-config` moves there what release 2.4 kept under `~/.config/llama.cpp/` and `~/.local/share/llama.cpp/` ([details](crates/config/README.md#migration-from-24)) and offers to copy a per-user `server.ini` and presets to the service (`pai config import`; [details](crates/config/README.md#through-the-manager-linux-windows)). Details: [docs/build-packaging.md](docs/build-packaging.md#the-deb-and-rpm-linux). A backend whose GPU, driver or runtime package is missing is skipped at start.

The dists install side by side as `<root>/<version>/linux` under `/mnt/data` (`TheRock`, `CUDA`, `VulkanSDK`; for another mountpoint pass `--dist-root <path>` to `scripts/00-install-prerequisites.sh` and `xtask configure`, which records it, or set `PRIVATE_AI_DIST_ROOT`), about 27 GB in all, and nothing is added to the environment: the scripts find the newest of each on every run. Everything generated lives under `build/linux/`, apart from the Windows artifacts under `build/windows/`. Details: [docs/build-prerequisites.md](docs/build-prerequisites.md#dist-layout-and-root).

## macOS

Apple Silicon only, Metal and Vulkan (through MoltenVK). **Untested**: written and compile-checked on Linux, not yet run on a Mac.

```bash
# 1. First time only: checks the Xcode Command Line Tools, installs cmake, ninja and
#    node with Homebrew, Rust (rustup), then the Vulkan SDK (into ~/dists, no sudo).
scripts/00-install-prerequisites.sh
#    Afterwards: cargo run -p xtask -- prereq

# 2. to 5., as on Linux:
cargo run -p xtask -- configure
cargo run -p xtask -- build
cargo run -p xtask -- package    # dist/private-ai-framework-{config,cli,llama}-v<version>-<llamaRelease>-arm64.pkg, -vulkan-v<sdkVersion>-arm64.pkg
cargo run -p xtask -- install    # sudo installer -pkg ... -target /, once per package
```

The packages install into `/opt/private-ai/` with a symlink per executable in `/usr/local/bin/`; they are unsigned and not notarized, and a `.pkg` has no dependencies, so install all four. Details: [docs/build-macos.md](docs/build-macos.md).

## Layout

All build artifacts live under `build\`, in a per-OS directory (`build\windows\`, `build/linux/`, `build/macos/`), so a single `rm -rf build\` returns the repo to a clean state.

```
<repo>/
├── build/
│   └── windows/
│       ├── .sccache/           ← sccache compile cache
│       ├── cargo-target/       ← cargo target dir of the Rust crates
│       ├── config-build.psd1   ← build-time config (generated by xtask configure)
│       ├── llama.cpp/          ← llama.cpp source clone (LlamaCppDir)
│       ├── llama.cpp-cmake/    ← cmake binary dir (out-of-source build)
│       ├── staging/            ← the tree the MSI installs (xtask package, removed unless --keep-staging)
│       └── wix-obj/            ← WiX intermediates (xtask package, transient)
├── dist/                    ← final installer .msi
├── installer/               ← the MSI source (.wxs + custom-action script), the WiX pin, dist-sources.psd1 (where external dists come from)
├── crates/                  ← the Rust crates: core/ (pai-core, shared), app/ (pai-app, the configurator's logic without a GUI), manager/ (pai-manager, the runtime service binary), config/ (pai-config, the GUI configurator), cli/ (pai-cli: pai, its command line)
├── patches/                 ← checked-in build workarounds (HIP clang runtime wrapper, per clang major)
├── resources/               ← icon generator (llama.ico is built on demand, gitignored)
├── scripts/                 ← helpers: 00-install-prerequisites (.ps1/.sh) and thin wrappers over the cargo phases
└── xtask/                   ← the build orchestrator (cargo run -p xtask -- <phase>): src/{common,linux,windows,macos}/, cmake/ (the llama.cpp CMake options: common.toml, <os>.toml)
```

## Configuration

Settings are split between build-time and runtime so the same scripts work both in-tree and from a system install.

| File | Where | Purpose |
|------|-------|---------|
| `config-build.psd1` | `build\windows\` (gitignored) | Build-time paths and flags. Generated by `xtask configure`. |
| `server.ini` | `%ProgramData%\private-ai\config\server@default.ini` (Linux: `/etc/private-ai/server@default.ini`; macOS: per user) | Machine-wide llama-server runtime (port, host, threads, threads-batch, models dir, max simultaneous models). Single `[Server]` section. Written by `pai-config`. |
| `presets.ini` | `%ProgramData%\private-ai\config\` (Linux: `/etc/private-ai/`; macOS: per user) | Per-model presets: one `[<model-id>]` section each with model path, ctx size, GPU layers, GPU distribution (device / split-mode / tensor-split), load mode, the image encoder's device, batch sizes, cache types, flash-attn, sampling, MoE expert offload, chat-template kwargs, speculative decoding (draft model / spec-type / draft device+layers). Consumed directly by `llama-server --models-preset`. Written by `pai-config`. |

> **GPU distribution** is edited as a table, not as free text: `--tensor-split` is a positional vector over the devices named by `--device`, so the configurator lists the detected GPUs with a checkbox and a weight each and derives both settings from that. The [configurator README](crates/config/README.md#gpu-distribution-table) has the details, including the two footguns it exists to make visible: a blank tensor-split over 2+ GPUs means *split by free VRAM*, not evenly; and the **image encoder (mmproj/CLIP) is placed on its own**: it follows the first `--device` entry, and with no device pinned llama.cpp puts it on the first GPU backend it finds, where it holds VRAM but only works on image requests; the preset's Vision GPU dropdown overrides both.

`config-build.psd1` keys:

| Section | Keys |
|---------|------|
| Paths | `DistRoot`, `LlamaCppDir`, `OpenSSLDir`, `HipPath`, `VsDevShell`, `WixExe` |
| Build | `GpuTargets`, `BuildType`, `CCompiler`, `CxxCompiler`, `Linker`, `BuildJobs` |

Runtime configs are created and edited with `pai-config` (the GUI) or headless with `pai` (`pai server`, `pai preset`, ...). The installer ships both in `bin\` next to `llama-server.exe`, with an optional Start Menu shortcut to the GUI ("Private AI Framework"). On Windows and Linux the server and preset files belong to the `private-ai-manager` service, and `pai-config` and `pai` change them through it. See [crates\config\README.md](crates/config/README.md).

## Phases

`xtask <phase>` below is short for `cargo run -p xtask -- <phase>`, run from the repo root; `cargo run -p xtask -- <phase> --help` lists each phase's options. `scripts\` holds only helpers: the prerequisites bootstrap and wrappers that run the same cargo commands.

| Command | Description |
|---------|-------------|
| `scripts\00-install-prerequisites.ps1` | Windows: idempotent toolchain bootstrapper, the one step that is not cargo (it installs what cargo needs). Installs missing winget packages (PowerShell 7+, OpenSSL, the .NET SDK) and upgrades present ones in a single UAC-elevated session, which also installs the newest stable ROCm/TheRock dist, CUDA Toolkit and Vulkan SDK into `<DistRoot>\<Name>\<Version>\windows`; installs or updates sccache from its latest GitHub release (sha256-verified, user scope); installs the WiX Toolset at its pinned version with its extensions (and reports a WiX EULA still to accept); fetches the llama.cpp clone and flags a rebuild when a newer release tag is available. Accepts `-DistRoot`. |
| `scripts/00-install-prerequisites.sh` | Linux and macOS: the system packages (apt, or Homebrew) and Rust, then `xtask prereq`. |
| `xtask prereq` | Linux and macOS: sccache and the newest stable GPU SDK dists, the llama.cpp release check. Accepts `--dist-root`, `--skip`. |
| `xtask configure` | Detects environment, verifies tools, writes the build config (`config-build.psd1` on Windows, `config-build.toml` elsewhere). Accepts `--llama-cpp-dir` and `--dist-root`. |
| `xtask build` | Builds **both** llama.cpp (CMake configure out-of-source into `build\<os>\llama.cpp-cmake\` + Ninja, checked out at the newest `vX.Y.Z` release tag: upstream also tags every merge to master `bNNNN`, and those nightlies are not candidates) **and** `pai-config`, `pai` and `pai-manager` (`cargo build --release` into `build\<os>\cargo-target\`). `xtask build llama` and `xtask build config` build one of them. Auto-clones llama.cpp into `build\<os>\llama.cpp\`. Uses sccache when available. |
| `xtask package` | Windows: stages into `build\windows\staging\`, then runs WiX to produce `dist\private-ai-framework-v<version>-<llamaBuild>-<arch>.msi` and validates it (`--wix`, `--keep-staging`). Linux: the `.deb` and `.rpm` of each component (`--deb`, `--rpm`); macOS: the `.pkg` of each component. |
| `xtask install` | Installs the newest package in `dist/` on this machine, on Linux and macOS the newest of each component or the files named (`--dry-run`). Windows: the MSI through `msiexec /qb` (progress bar and the UAC prompt, all features, the recorded install directory; a downgrade removes the installed version first). Linux and macOS: the `.deb`, `.rpm` or `.pkg` through the package manager (`--force` installs while a program of the packages runs: on Linux one in `/usr/bin` or holding a library of `/usr/lib/private-ai`, the services aside, which the packages restart; on macOS one from `/opt/private-ai`; the MSI stops them itself). |
| `crates\config\` | Rust GUI configurator `pai-config` for `server.ini` / `presets.ini`, opencode/Claude Code integrations, and llama-server start/stop. See [crates\config\README.md](crates/config/README.md). |
| `crates\cli\` | `pai`, the configurator's command line (package `pai-cli`): the same operations headless. Documented with the GUI in [crates\config\README.md](crates/config/README.md). |

## Packaging

`xtask package` produces a Windows Installer package at `dist\private-ai-framework-v<version>-<llamaRelease>-<arch>.msi` (e.g. `private-ai-framework-v2.99.0-v0.9.0-x64.msi`: the framework version first, the bundled llama.cpp release second). The package:

- Installs llama.cpp binaries to `C:\Program Files\private-ai\` by default, the directory choosable (`pai-config.exe`, `pai.exe`, `pai-manager.exe` and `llama.ico` go into `bin\` next to `llama-server.exe`).
- Installs the `private-ai-manager` service (LocalSystem, started at boot), which runs each llama-server instance as its own service (`llama.cpp_<id>`, under the virtual account `NT SERVICE\llama.cpp_<id>`) and serves the configurator over the named pipe `\\.\pipe\private-ai-manager`: any interactive user may start and stop the server, only a member of Administrators may change its settings. The server and preset files are in `%ProgramData%\private-ai\config\`, the instance's log in `%ProgramData%\private-ai\logs\llama-server@default.log`; the MSI also creates `%ProgramData%\private-ai\models\`, which the instances can read (a models folder in a user's profile is not readable by them). These folders outlive an uninstall. Written and type-checked, not yet run on Windows ([installer\AGENTS.md](installer/AGENTS.md)).
- Is self-contained for the GPU backends: the ROCm and CUDA runtime libraries the backends load (with rocBLAS's and hipBLASLt's kernels for the targeted GPU architectures) are copied out of the dists the build linked against, into `bin\` and `.kpack\`. The Vulkan loader is bundled too. The target machine needs the GPU drivers and the Visual C++ Redistributable; nothing is downloaded at install time.
- Optionally adds `bin\` to the system PATH (on by default), so `pai` runs from any new terminal.
- Optionally creates a Start Menu shortcut ("Private AI Framework" → `pai-config.exe`).
- No install-time configuration page: all runtime configuration (server settings and per-model presets) is done post-install with `pai-config`.
- Stops a running `pai-config`, `pai` and `llama-server` before it replaces or removes their files (gracefully first with `pai.exe control stop-and-close`, so the conversation snapshot is saved: [installer\AGENTS.md](installer/AGENTS.md)).
- Supports upgrades (removes the previous version first; refuses a downgrade), release 2.4's `llama.cpp-framework` included: an install directory chosen in 2.4 is kept, the old default `C:\Program Files\llama.cpp\` becomes `C:\Program Files\private-ai\`, and `pai-config` moves the 2.4 user tree `%LOCALAPPDATA%\llama.cpp\` at its first start. On a machine with a previous installation that is not this MSI, it stops before changing anything and asks to uninstall that one first from Settings > Apps (answering No to its question keeps the user data).
- Is listed in Settings > Apps (and Programs and Features). Uninstalling from there removes the manager's and the instances' services and preserves `%LOCALAPPDATA%\private-ai\` (settings, benchmark results, an earlier install's configs and logs) unless the user ticks the box that offers to delete it; a silent uninstall always preserves it.
