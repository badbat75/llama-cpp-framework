# Private AI Framework

> **Windows first**: the full pipeline (build, installer) targets Windows 10/11 and ends in two MSIs, llama.cpp's own and the framework's, which requires it (not yet built or run on Windows since that split). Linux ends in eight `.deb` and eight `.rpm` packages, the engine `llama.cpp` apart from the framework (see [Linux](#linux)), macOS on Apple Silicon in five `.pkg`, the engine's two and the framework's three (see [macOS](#macos), type-checked only).

A build orchestrator, run with cargo (`cargo run -p xtask -- <phase>`, the crate in `xtask/`), that builds and packages [llama.cpp](https://github.com/ggerganov/llama.cpp) with multi-GPU backend support (CUDA, Vulkan, ROCm/HIP on Windows and Linux; Metal and Vulkan on macOS) and MSI packaging (WiX Toolset) on Windows. The chat UI built into `llama-server` is used as the frontend. `pai-bff`, a local service with its own web UI (`fe/`), ships as a service on Linux and Windows; its web UI shows, read-only, the framework version and the manager's instances with their state so far.

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
- Self-contained packages: two MSIs, llama.cpp's and the framework's (WiX, upgrades, downgrade refused, ICE-validated), split `.deb` and `.rpm` packages under `/usr`, split `.pkg` on macOS; the ROCm, CUDA and Vulkan runtimes the backends load are packaged too, so a target machine needs only the GPU driver.
- Out-of-tree llama.cpp patches in `patches/llama.cpp/`, applied on every build and reverted before the next checkout, each with its source, digest and the condition to drop it: a trimmed MTP draft vocabulary for Qwen3.5/3.8, and the fix of the MTP draft graph's reallocations after an image.

**Configurator** ([crates/config/README.md](crates/config/README.md))

- Five tabs (Server, Models, Benchmark, Integrations, Settings), a tray icon, a live log viewer, and the HW Monitor window (GPU, VRAM and its spill into system memory, PCIe traffic on NVIDIA, CPU, memory, disk).
- Presets edited as forms with every llama-server knob explained in place; GPU distribution (`device`, `split-mode`, `tensor-split`) as a table of detected GPUs, tensor placement (`override-tensor`) as ordered rules, the vision encoder's GPU on its own.
- Hugging Face search and download that creates the preset and recommends a quantization that fits this machine's GPUs.
- Conversation snapshots: each model's KV cache saved on stop and restored on the next start, so a restart does not re-read a long conversation.
- Integrations: the OpenCode provider (one model per enabled preset) and a Claude Code environment snippet; log rotation; start at login; start llama-server when the system starts (Linux, Windows: the instance's service enabled, `pai server boot on|off`) or, on macOS, when the configurator launches.
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
- [Node.js](https://nodejs.org/) LTS with npm: `xtask configure` and `xtask build` stop without it (the build makes the web UI in `fe\` and a missing `resources\llama.ico` with it)

Installed/updated from its GitHub release (winget lags upstream by days) into `%LOCALAPPDATA%\Programs\sccache`, also by `scripts\00-install-prerequisites.ps1`:
- [sccache](https://github.com/mozilla/sccache) (compiler cache for the C/C++ and HIP compiles)

Installed as a .NET global tool at the version pinned in `installer\wix-toolset.psd1`, with its UI and Util extensions, also by `scripts\00-install-prerequisites.ps1`:
- [WiX Toolset](https://wixtoolset.org/) 7 (builds the MSIs). WiX 7 asks each user to accept its EULA once before it builds anything (Open Source Maintenance Fee: organizations above USD 10,000 yearly revenue owe the fee); the scripts never accept it for you: read it, then run `wix eula accept wix7`.

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

# 4. Produce the Windows installers in dist\: llama.cpp's MSI and the framework's (package llama|framework for one)
cargo run -p xtask -- package

# 5. Install them on this machine, llama.cpp's first (UAC prompt; upgrade, repackage or downgrade alike)
cargo run -p xtask -- install
```

## Linux

Debian/Ubuntu: the same cargo phases end at a working `llama-server` with the CUDA, Vulkan and HIP backends and `pai-config` and `pai` beside it, packaged as eight `.deb` and eight `.rpm` packages: the framework's four at the framework's version, the engine `llama.cpp` and its three GPU runtimes at their own.

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

# 4. Package: dist/private-ai-framework-{config,cli,bff,core}-v<version>-amd64.deb,
#    dist/llama.cpp-v<release>+pai<N>-amd64.deb, dist/llama.cpp-{rocm,cuda,vulkan}-v<sdkVersion>-amd64.deb,
#    and the same as -x86_64.rpm (llama.cpp must be on a release tag)
cargo run -p xtask -- package

# 5. Install (or upgrade, or reinstall) the newest package of each component in dist/: the .deb
#    or the .rpm, as the distribution takes (sudo). Name the files to install only those.
cargo run -p xtask -- install
```

The packages (about 1.7 GiB in all, 3.8 GiB installed) are self-contained, so the target machine needs the GPU drivers and nothing else:

| Package | Holds | Version |
|---|---|---|
| `private-ai-framework-config` | `pai-config`, the GUI, and its data (requires `cli` and `core` at the same version, `llama.cpp` at least at the release it was built against) | framework, `2.99.1` |
| `private-ai-framework-cli` | `pai`, the command line, and the benchmark method (requires `core` at the same version and `llama.cpp` at least; no graphics library) | framework |
| `private-ai-framework-bff` | `pai-bff`, the web service, its systemd unit and its user (requires `core` at the same version) | framework |
| `private-ai-framework-core` | `pai-manager`, its systemd unit, the D-Bus and polkit policies, the service user and the text every program prints (requires `llama.cpp` at least) | framework |
| `llama.cpp` | The engine: llama.cpp's programs and libraries, the ggml backends included, and its instance unit `llama.cpp@.service`, which runs only with the framework's `pai-manager` installed | `1:<release>+pai<N>`, `1:1.0.0+pai1` |
| `llama.cpp-rocm` | The ROCm runtime libraries (`lib/`) | ROCm SDK |
| `llama.cpp-cuda` | The CUDA and NCCL runtime libraries (`lib/`) | CUDA + NCCL SDK |
| `llama.cpp-vulkan` | The Vulkan loader (`lib/`) | Vulkan SDK |

`llama.cpp` recommends the three runtimes and refuses any other version of them; leave out the ones the machine has no use for. A newer `llama.cpp` installs without rebuilding the framework. A headless server installs `core`, `cli`, `llama.cpp` and its runtimes, without `config` and the graphics libraries it pulls in. `config`, `llama.cpp` and the runtimes upgrade in place the packages of release 2.4 and earlier, `llama-cpp-framework` and `llama-cpp-framework-<component>`, which they replace (`llama.cpp` replaces `-llama`); `llama.cpp` refuses to sit beside the distribution's own (`llama.cpp-tools` on Ubuntu, `llama-cpp` on Fedora). They install the executables in `/usr/bin/` (`llama-server`, `pai-config`, `pai`, the other llama.cpp tools), the libraries and runtimes in `/usr/lib/private-ai/`, a desktop entry that puts the configurator ("Private AI Framework") in the applications menu, and, in `core`, the `private-ai-manager` service (enabled and started on install) that runs llama-server as the systemd instance `llama.cpp@default` of `llama.cpp`'s unit (logging to the journal: `journalctl -u llama.cpp@default`; started at boot only once enabled, the configurator's "start llama-server when the system starts" or `pai server boot on`), and, in `bff`, the `private-ai-bff` service that serves the web UI on `http://127.0.0.1:8590` (enabled and started on install, as the system user `private-ai-bff`). An upgrade or removal of a package first stops the services it holds (`llama.cpp` its running instances) and an upgrade starts again what was running. The configurator is its client: the server and preset files are the service's, in `/etc/llama.cpp/` (filled at install from the defaults `llama.cpp` ships, each only when missing, never over a file), changed through it with polkit asking for an administrator's password; `settings.ini` stays per user in `~/.config/private-ai/` (benchmark runs in `~/.local/share/private-ai/`). At its first start `pai-config` moves there what release 2.4 kept under `~/.config/llama.cpp/` and `~/.local/share/llama.cpp/` ([details](crates/config/README.md#migration-from-24)) and offers to copy a per-user `server.ini` and presets to the service (`pai config import`; [details](crates/config/README.md#through-the-manager-linux-windows)). Details: [docs/build-packaging.md](docs/build-packaging.md#the-deb-and-rpm-linux). A backend whose GPU, driver or runtime package is missing is skipped at start.

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
cargo run -p xtask -- package    # five .pkg: llama.cpp and its Vulkan runtime, the framework's config, cli, core
cargo run -p xtask -- install    # sudo installer -pkg ... -target /, once per package, the engine's first
```

llama.cpp installs into `/opt/llama.cpp/` and the framework into `/opt/private-ai/`, with a symlink per executable in `/usr/local/bin/`; the packages are unsigned and not notarized, and a `.pkg` has no dependencies, so install all five (the Vulkan one is optional: without it Metal runs). The packaging is type-checked only, never run on a Mac, and a release 2.4 install in `/opt/llama.cpp/` is not removed. Details: [docs/build-macos.md](docs/build-macos.md).

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
│       └── staging/            ← the trees the MSIs install, llama/ and framework/ (xtask package, removed unless --keep-staging), and each one's WiX intermediates (transient)
├── dist/                    ← the finished installers (.msi)
├── installer/               ← the two MSI sources (.wxs + custom-action scripts), the WiX pin, dist-sources.psd1 (where external dists come from)
├── crates/                  ← the Rust crates: core/ (pai-core, shared), app/ (pai-app, the configurator's logic without a GUI), manager/ (pai-manager, the runtime service binary), config/ (pai-config, the GUI configurator), cli/ (pai-cli: pai, its command line), bff/ (pai-bff, the web UI's service)
├── fe/                      ← the web UI pai-bff embeds (React + Vite, TypeScript), built by xtask build
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
| `server.ini` | `%ProgramData%\llama.cpp\config\server@default.ini` (Linux: `/etc/llama.cpp/server@default.ini`; macOS: per user) | Machine-wide llama-server runtime (port, host, threads, threads-batch, models dir, max simultaneous models). Single `[Server]` section. Written by `pai-config`. |
| `presets.ini` | `%ProgramData%\llama.cpp\config\` (Linux: `/etc/llama.cpp/`; macOS: per user) | Per-model presets: one `[<model-id>]` section each with model path, ctx size, GPU layers, GPU distribution (device / split-mode / tensor-split), load mode, the image encoder's device, batch sizes, cache types, flash-attn, sampling, MoE expert offload, chat-template kwargs, speculative decoding (draft model / spec-type / draft device+layers). Consumed directly by `llama-server --models-preset`. Written by `pai-config`. |

> **GPU distribution** is edited as a table, not as free text: `--tensor-split` is a positional vector over the devices named by `--device`, so the configurator lists the detected GPUs with a checkbox and a weight each and derives both settings from that. The [configurator README](crates/config/README.md#gpu-distribution-table) has the details, including the two footguns it exists to make visible: a blank tensor-split over 2+ GPUs means *split by free VRAM*, not evenly; and the **image encoder (mmproj/CLIP) is placed on its own**: it follows the first `--device` entry, and with no device pinned llama.cpp puts it on the first GPU backend it finds, where it holds VRAM but only works on image requests; the preset's Vision GPU dropdown overrides both.

`config-build.psd1` keys:

| Section | Keys |
|---------|------|
| Paths | `DistRoot`, `LlamaCppDir`, `OpenSSLDir`, `HipPath`, `VsDevShell`, `WixExe` |
| Build | `GpuTargets`, `BuildType`, `CCompiler`, `CxxCompiler`, `Linker`, `BuildJobs` |

Runtime configs are created and edited with `pai-config` (the GUI) or headless with `pai` (`pai server`, `pai preset`, ...). The framework's installer ships both in its `bin\` (`llama-server.exe` is in llama.cpp's own install), with an optional Start Menu shortcut to the GUI ("Private AI Framework"). On Windows and Linux the server and preset files belong to the `private-ai-manager` service, and `pai-config` and `pai` change them through it. See [crates\config\README.md](crates/config/README.md).

## Phases

`xtask <phase>` below is short for `cargo run -p xtask -- <phase>`, run from the repo root; `cargo run -p xtask -- <phase> --help` lists each phase's options. `scripts\` holds only helpers: the prerequisites bootstrap and wrappers that run the same cargo commands.

| Command | Description |
|---------|-------------|
| `scripts\00-install-prerequisites.ps1` | Windows: idempotent toolchain bootstrapper, the one step that is not cargo (it installs what cargo needs). Installs missing winget packages (PowerShell 7+, OpenSSL, the .NET SDK, Node.js LTS) and upgrades present ones in a single UAC-elevated session, which also installs the newest stable ROCm/TheRock dist, CUDA Toolkit and Vulkan SDK into `<DistRoot>\<Name>\<Version>\windows`; installs or updates sccache from its latest GitHub release (sha256-verified, user scope); installs the WiX Toolset at its pinned version with its extensions (and reports a WiX EULA still to accept); fetches the llama.cpp clone and flags a rebuild when a newer release tag is available. Accepts `-DistRoot`. |
| `scripts/00-install-prerequisites.sh` | Linux and macOS: the system packages (apt, or Homebrew) and Rust, then `xtask prereq`. |
| `xtask prereq` | Linux and macOS: sccache and the newest stable GPU SDK dists, the llama.cpp release check. Accepts `--dist-root`, `--skip`. |
| `xtask configure` | Detects environment, verifies tools, writes the build config (`config-build.psd1` on Windows, `config-build.toml` elsewhere). Accepts `--llama-cpp-dir` and `--dist-root`. |
| `xtask build` | Builds **both** llama.cpp (CMake configure out-of-source into `build\<os>\llama.cpp-cmake\` + Ninja, checked out at the newest `vX.Y.Z` release tag: upstream also tags every merge to master `bNNNN`, and those nightlies are not candidates) **and** the web UI in `fe\` (`npm ci`, `npm run build`), then `pai-config`, `pai`, `pai-manager` and `pai-bff` (`cargo build --release` into `build\<os>\cargo-target\`). `xtask build llama` and `xtask build framework` build one of them; `pai-config` and `pai` are also copied beside `llama-server` for the build tree (and the macOS package). Auto-clones llama.cpp into `build\<os>\llama.cpp\`. Uses sccache when available. |
| `xtask package` | Windows: stages each product into `build\windows\staging\<product>\`, then runs WiX to produce `dist\llama.cpp-v<release>-<arch>.msi` and `dist\private-ai-framework-v<version>-<arch>.msi` and validates each (`--wix`, `--keep-staging`; `xtask package llama` or `framework` builds one). Linux: the `.deb` and `.rpm` of each component (`--deb`, `--rpm`), the framework's executables from `cargo-target/release/`; `xtask package llama` or `xtask package framework` packages one product and leaves the other's files in `dist/`; macOS: the `.pkg` of each component, the same `llama` or `framework` choice. |
| `xtask install` | Installs the newest package in `dist/` on this machine, on Linux and macOS the newest of each component or the files named (`--dry-run`). Windows: the newest llama.cpp MSI, then the newest framework MSI, or the files given (at most one of each, llama.cpp's first), each through `msiexec /qb` (progress bar and the UAC prompt, all features, the recorded install directory; a downgrade removes the installed version of that product first, llama.cpp's keeping the instance services; type-checked only, not run on Windows). Linux and macOS: the `.deb`, `.rpm` or `.pkg` through the package manager (`--force` installs while a program of the packages runs: on Linux a llama-server outside the framework's services, run from `/usr/bin` or holding a library of `/usr/lib/private-ai` (the services are stopped and started again by their packages, other programs are not checked); on macOS one from `/opt/llama.cpp` or `/opt/private-ai`, the engine's `.pkg` laid down first; the framework's MSI stops them itself). |
| `crates\config\` | Rust GUI configurator `pai-config` for `server.ini` / `presets.ini`, opencode/Claude Code integrations, and llama-server start/stop. See [crates\config\README.md](crates/config/README.md). |
| `crates\cli\` | `pai`, the configurator's command line (package `pai-cli`): the same operations headless. Documented with the GUI in [crates\config\README.md](crates/config/README.md). |
| `crates\bff\`, `fe\` | `pai-bff`, a local HTTP service (loopback only, `127.0.0.1:8590` by default) serving the web UI built from `fe\` (React + Vite, TypeScript), embedded in the binary. A service on Linux (`private-ai-bff.service`) and Windows (`private-ai-bff`), none on macOS. See [crates\bff\README.md](crates/bff/README.md). |

## Packaging

`xtask package` produces two Windows Installer packages: **llama.cpp's own MSI**, `dist\llama.cpp-v<release>-<arch>.msi` (e.g. `llama.cpp-v1.0.0-x64.msi`, versioned by the llama.cpp release), and **the framework's MSI**, `dist\private-ai-framework-v<version>-<arch>.msi` (e.g. `private-ai-framework-v2.99.1-x64.msi`), each with its own identity, upgraded on its own. Neither has been compiled by WiX or run on Windows since the split: written, text-checked and type-checked only ([installer\AGENTS.md](installer/AGENTS.md), which holds the test plan). Install llama.cpp's first: the framework's MSI refuses to install without llama.cpp at the release it was built against or newer.

llama.cpp's MSI:

- Installs llama.cpp to `C:\Program Files\llama.cpp\` by default, the directory choosable, and registers it in `HKLM\Software\llama.cpp`, where the framework finds `llama-server.exe`.
- Is self-contained for the GPU backends: the ROCm and CUDA runtime libraries the backends load (with rocBLAS's and hipBLASLt's kernels for the targeted GPU architectures) are copied out of the dists the build linked against, into `bin\` and `.kpack\`. The Vulkan loader is bundled too. The target machine needs the GPU drivers and the Visual C++ Redistributable; nothing is downloaded at install time.
- Creates `%ProgramData%\llama.cpp\` with `config\` (the instances' server and preset files, which users reach only through the manager), `logs\` (each instance's `llama-server@<id>.log`), `models\` and `state\` (the folders an instance uses when its server file sets no models folder or snapshot folder: one in a user's profile is not readable by the instances; a save from `pai-config` or `pai` still writes the user's `~\.llama.cpp` as the models folder, open in `PLAN.md` step 11), and copies its defaults into `config\` only when missing (`server@default.ini` with the port and host, an empty `presets.ini`).
- Removes release 2.4's `llama.cpp-framework` first, whose folder and registry key it takes over.
- Stops the running instances before a repair or upgrade replaces its files and starts them again afterwards.
- On a machine with a previous llama.cpp installation that is not an MSI, stops before changing anything and asks to uninstall that one first.
- Uninstalling it deletes the instances' services and keeps `%ProgramData%\llama.cpp\` unless the user ticks the box that offers to delete it; a silent uninstall and every upgrade keep it.

The framework's MSI:

- Installs `pai-config.exe`, `pai.exe`, `pai-manager.exe`, `pai-bff.exe` and `llama.ico` to `C:\Program Files\private-ai\bin\` by default, the directory choosable.
- Installs the `private-ai-manager` service (LocalSystem, started at boot), which runs each llama-server instance as its own service (`llama.cpp_<id>`, under the virtual account `NT SERVICE\llama.cpp_<id>`, started at boot only when its start type is automatic: the configurator's "start llama-server when the system starts") and serves the configurator over the named pipe `\\.\pipe\private-ai-manager`: any interactive user may start and stop the server, only a member of Administrators may change its settings.
- Installs the `private-ai-bff` service (`pai-bff.exe service`, started at boot, under the virtual account `NT SERVICE\private-ai-bff`, restarted after a failure), which serves the web UI on `http://127.0.0.1:8590`; its `bff.ini` goes in `%ProgramData%\private-ai\config\`, which outlives an uninstall.
- Optionally adds `bin\` to the system PATH (on by default), so `pai` runs from any new terminal.
- Optionally creates a Start Menu shortcut ("Private AI Framework" → `pai-config.exe`).
- No install-time configuration page: all runtime configuration (server settings and per-model presets) is done post-install with `pai-config`.
- Stops a running `pai-config` and `pai`, and the instance `default`, before it replaces or removes their files (gracefully first with `pai.exe control stop-and-close`, so the conversation snapshot is saved: [installer\AGENTS.md](installer/AGENTS.md)).
- Supports upgrades (removes the previous version first; refuses a downgrade). From release 2.4: install llama.cpp's MSI first (it removes 2.4), then this one, into `C:\Program Files\private-ai\` (a directory chosen in 2.4 is not carried over); `pai-config` moves the 2.4 user tree `%LOCALAPPDATA%\llama.cpp\` at its first start. On a machine with a previous installation that is not this MSI, it stops before changing anything and asks to uninstall that one first from Settings > Apps (answering No to its question keeps the user data).
- Is listed in Settings > Apps (and Programs and Features). Uninstalling from there removes the manager's, the web service's and the instances' services and preserves `%LOCALAPPDATA%\private-ai\` (settings, benchmark results, an earlier install's configs and logs) unless the user ticks the box that offers to delete it; a silent uninstall always preserves it.

Which installers a release attaches is to settle (`PLAN.md` step 11).
