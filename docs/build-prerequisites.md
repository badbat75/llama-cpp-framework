---
title: Build prerequisites
type: reference
updated: 2026-11-14
---

# Build prerequisites

What a machine holds before `xtask configure` runs: where the GPU SDKs live and which one a build
takes, and what the bootstrap scripts install on Windows and Linux. macOS is on
[its own page](build-macos.md). The phases and what breaks are in
[Build pipeline](build-pipeline.md).

## Dist layout and root

**Every GPU SDK lives at `<root>/<Name>/<Version>/<os>`, for example `E:\TheRock\10.1.0\windows` or
`/mnt/data/TheRock/10.1.0/linux`.** The OS level exists because a drive can be shared by the Windows
and Linux sides of one machine, and neither build sees the other's files.

The dist root resolves in this order, on every OS:

1. An explicit `--dist-root` on `xtask prereq` and `xtask configure` (`-DistRoot` on the PowerShell
   script).
2. `PRIVATE_AI_DIST_ROOT`.
3. The root `xtask configure` recorded in `config-build.*`.
4. The default in `installer/dist-sources.toml` (`.psd1` for PowerShell). On macOS a leading `~/` is
   the home directory.

`xtask configure` records the resolved root, so it is given once. `xtask build` and `xtask package`
use the recorded root and ignore `PRIVATE_AI_DIST_ROOT` while one is recorded. To move them, re-run
`xtask configure`.

**A dist counts only with its marker and the files the build consumes.** The marker is written after
a successful install, so an interrupted one is never taken. Among those that count, the highest
version wins, compared numerically.

| Dist | Marker | Windows needs | Linux needs |
| --- | --- | --- | --- |
| `TheRock` | `.therock-version` | `lib\llvm\bin\clang.exe`, `lib\cmake\hip\hip-config.cmake`, `bin\rocblas\library` | `lib/llvm/bin/clang++`, `lib/cmake/hip/hip-config.cmake` |
| `CUDA` | `.cuda-version` | `bin\nvcc.exe`, `lib\x64\cudart_static.lib`, `lib\x64\cublas.lib` | `bin/nvcc`, `lib64/libcudart.so`, `lib64/libcublas.so` |
| `NCCL` | `.nccl-version` | n/a | `lib/libnccl.so`, `include/nccl.h` |
| `VulkanSDK` | `.vulkan-version` | `Lib\vulkan-1.lib`, `Bin\glslc.exe` | `x86_64/include/vulkan/vulkan.h`, `x86_64/bin/glslc`, the loader |

On Windows the dist also holds `Runtime\vulkan-1.dll`, the loader the MSI bundles. It is no part of
what makes a dist complete: a build never needs it, and `xtask package` is what refuses a Vulkan
build without it.

The finders are `xtask/src/<os>/dists.rs` and `Find-Dist` in `scripts\dists.ps1`. The directory
`VULKAN_SDK` names is the dist itself on Windows, `<Version>/linux/x86_64` on Linux and
`<Version>/macos/macOS` on macOS. The Linux loader is in `lib/VulkanLoader/lib` on 1.4.3xx SDKs and
in `lib/` on older ones, and `vulkan_loader` handles both.

**The layout wins over the environment, with these limits.**

- `CUDA_PATH` and `VULKAN_SDK` never select a dist, and a stale console value is overridden for the
  build. Neither does `HIP_PATH` on Windows.
- On Linux, `xtask configure` takes an explicit `HIP_PATH` or `ROCM_PATH` over the layout when it
  holds `lib/llvm/bin/clang++`.
- When the layout holds no CUDA Toolkit or Vulkan SDK, the build warns and leaves detection to
  CMake, which may find another install.
- ROCm has no such fallback: an empty recorded ROCm path fails the build on Windows and Linux.

**Dists sit side by side and nothing deletes one.** An upgrade lands beside its predecessor, and the
report lists the older ones: each is a working rollback, and removing it is your call. A Windows
ROCm dist takes about 25 GB, a Linux one about 22 GB.

**OpenSSL and the tools are not on the layout.** OpenSSL stays a system library on Windows, in
winget's `Program Files\OpenSSL-Win64`. `xtask configure` also tries `C:\OpenSSL-Win64`, then the
directory above an `openssl.exe` on PATH. Visual Studio, cmake, ninja, git, node, Rust, the .NET
SDK, WiX, PowerShell 7 and sccache stay where their own installers put them.

<details>
<summary>Windows: why the dists are locked, and what that leaves open</summary>

The layout is trusted. The build runs the compiler found there, the elevated session unpacks into
it, and the active ROCm dist is on the machine PATH.

So the elevated session locks each Windows dist with `Protect-DistDir`: the ones it installs, and
the ones it finds writable by Everyone, Authenticated Users or Users.

- The owner becomes Administrators, because an unelevated owner could rewrite the ACL.
- Inheritance goes off.
- Administrators and SYSTEM get full control, Users read and execute, inherited by the whole tree.

Only the `windows` directories are touched. `<DistRoot>\<Dir>`, its `.downloads` and the `linux`
dists keep the ACL their owner gave them. Only directories named like a version are locked, so a
planted directory name stays data. The report flags a dist still unlocked.

The remaining risk: whoever can write `<DistRoot>\<Dir>` can still create a new version directory
with a marker in it, and `Find-Dist` would take it as the newest. The ROCm tarball has no published
checksum, so only HTTPS and the expected size vouch for it.

</details>

## Prerequisites

**node and npm are required on every OS**: `xtask configure` counts a missing one as a gap and fails,
and `xtask build` builds the [web UI](build-compiling.md#the-web-ui-and-pai-bff) with them. The
bootstrap installs them (winget's Node.js LTS on Windows, apt's `nodejs npm` on Linux, Homebrew's
`node` on macOS).

**The bootstrap is idempotent, and every GPU dist converges to the newest stable release its vendor
publishes.** `installer/dist-sources.*` says where each is looked up and holds no versions. A source
that cannot be reached keeps what is installed and the report says so. Nothing is downgraded or
deleted. WiX is the one pin: `installer/wix-toolset.psd1`.

| Item | Newest version from | Checked by |
| --- | --- | --- |
| ROCm | AMD's tarball indexes, strict `X.Y.Z` match | Windows: expected size. Linux: none published, a truncated file fails in the extract |
| CUDA Toolkit | Highest `redistrib_<X.Y.Z>.json` on NVIDIA's redist index | sha256 of each archive, from that manifest |
| NCCL (Linux) | PyPI's JSON API for `nvidia-nccl-cu<major>` | sha256 from PyPI |
| Vulkan SDK | LunarG's `sdk/latest/<os>.txt` | Windows: size and Authenticode. Linux, macOS: sha256 from `files.json`, no checksum means no install |
| sccache | GitHub `/releases/latest` | sha256 of the API's `digest`; Windows falls back to the `.sha256` sidecar |

**ROCm comes from AMD's TheRock tarball, because the classic HIP SDK installer is discontinued. Its
URL is found, never composed.** The host depends on the version, and a wrong host answers 403, which
reads like "not published". The strict match keeps out `-tests-` builds and dated nightlies. The
tarball is the multiarch one, with kernels for every gfx family, so one install serves any GPU.

**CUDA is assembled from NVIDIA's redist archives, not its installer.** The archives merge into one
prefix with `--strip-components=1`. That needs no driver and no root on Linux, and works with any
prefix. NVIDIA's Windows installer cannot be given a prefix per version and pulls in the display
driver unless told otherwise.

**sccache's install directory has no version in it, on purpose.** `xtask build` passes that path to
CMake as the compiler launcher. A moving path changes every compile command, so ninja would re-run
all of them after each upgrade. sccache tracks the latest release, since a misbehaving cache costs a
cold build and not a broken runtime. It comes from GitHub because winget and distro packages lag
behind.

**An existing Rust toolchain is never updated**, because a rustc upgrade recompiles the whole
dependency tree. `rustup update` stays your call.

**The llama.cpp check only compares.** It fetches the clone and compares its tag with the newest
release, by the same selection `xtask build` makes. It never pulls, because the clone sits on a
detached tag.

### Windows prerequisites

`scripts\00-install-prerequisites.ps1` stays native PowerShell. It runs under Windows PowerShell 5.1
before Rust exists, and it drives one UAC-elevated session. It reads `config-build.psd1` directly,
so it works on a fresh machine before `xtask configure` has run.

| Item | How it is installed | Scope |
| --- | --- | --- |
| PowerShell 7, OpenSSL, .NET SDK 10, Node.js LTS (with npm) | winget, installed or upgraded, plus links for OpenSSL's `lib\VC\x64\MD\*.lib` into `lib\` | Elevated |
| ROCm | Extracted to `<DistRoot>\TheRock\<version>\windows` from `.downloads`; a partial download resumes | Elevated |
| CUDA Toolkit | Redist archives in `<DistRoot>\CUDA\.downloads\<X.Y.Z>`; a re-run reuses finished downloads | Elevated |
| Vulkan SDK | LunarG's installer, headless, into `<DistRoot>\VulkanSDK\<version>\windows`; then the loader copied from System32, where the SDK's `Helpers\VulkanRT.exe` leaves it, into its `Runtime\`, also for an SDK already installed | Elevated |
| sccache | `%LOCALAPPDATA%\Programs\sccache\sccache.exe`, its directory added to the user PATH | User |
| WiX 7.0.1 and extensions | `dotnet tool`, moved to the pin with `--allow-downgrade`; `WixToolset.UI.wixext`, `WixToolset.Util.wixext` | User |
| Rust | winget's `Rustlang.Rustup`, when no cargo is found | User |

**WiX v7 needs `wix eula accept wix7` once per user.** The script never runs it. The report names it
as the missing step, and the next run adds the extensions. Its `WiX Toolset` row shows the version.

**ROCm has the only machine-wide settings.** The elevated session sets `HIP_PATH` to the active dist
and puts its `bin` and `lib\llvm\bin` on the machine PATH.

- A llama-server run from the build tree finds the runtime through them. The installed one carries
  its own.
- A version already in the layout is reached by moving `HIP_PATH`, with no download.
- The session deletes machine-wide copies of the compile-time vars ([why](build-compiling.md#windows)).
- Every other ROCm entry leaves PATH, because `amdhip64_*.dll` reachable from two PATH directories
  is an ambiguous load order and a classic silent crash.
- PATH goes through the raw registry. `setx /M` truncates it at 1024 characters, and
  `[Environment]::SetEnvironmentVariable` downgrades `REG_EXPAND_SZ` to `REG_SZ`.

| ROCm situation | What the run does |
| --- | --- |
| Installed version does not parse (`unknown`, a hand-installed nightly) | Replaces it |
| Installed version is newer than the newest stable | Keeps it |
| Active dist outside the layout, for example `C:\TheRock\<version>` | Leaves it, converges inside the layout, and names it in the report |
| llama-server runs and the new version's directory exists | Skips the install: a wipe and re-extract must not run under a server |
| llama-server runs and the version is new | Installs: it lands beside the running one |

Copy an outside dist to `<DistRoot>\TheRock\<version>\windows` with its marker to save the 4.6 GB
download. A wipe only ever clears an incomplete tree of the same version. Below about 40 GB free,
the script warns before the download.

**CUDA and Vulkan register nothing that the build reads.** The toolkit gets no `CUDA_PATH`, no PATH
entry and no driver change. Its tree holds `bin\nvcc.exe`, import libs in `lib\x64` and runtime DLLs
in `bin\x64`. LunarG's installer sets a machine `VULKAN_SDK` and a PATH entry of its own, and the
build reads neither.

**The report warns on anything that can shadow the runtime:**

- a legacy HIP SDK under `Program Files\AMD\ROCm`;
- a user-level `HIP_PATH` that differs from the machine one;
- any persistent `LLVM_PATH`, `HIP_DEVICE_LIB_PATH` or `HIP_PLATFORM`;
- `amdhip64_*.dll` in more than one PATH directory.

That last check sets aside the installed framework's `bin\`, found through `HKLM\Software\private-ai`
`InstallDir`, and the driver's copy under the Windows directory. The installed set is
self-contained: an exe resolves it from its own directory before PATH, and PATH never decides
`amdhip64_7.dll` for another exe.

**The report ends with the next steps.** It recommends `xtask build` and `xtask package` for a newer
llama.cpp, ROCm, CUDA or Vulkan. `xtask configure` comes first when the ROCm dist changed or the
config names another root, with `--dist-root` when configure would not resolve the same root by
itself.

<details>
<summary>How the elevated session stays safe, and sccache details</summary>

**The batch goes through a temporary file, not `-EncodedCommand`.** Base64 of UTF-16 inflates it
about 2.7 times, and with the ROCm, CUDA and Vulkan parts it does not fit Windows' 32,767-character
command line.

**TEMP is user-writable, so the file is never run by path.** The elevated loader reads its bytes
once, compares their SHA256 with the hash on its own command line, and runs the verified bytes from
memory. A swap after the write exits 3.

**What was verified is what runs.** Every downloaded installer must pass Authenticode, signed by its
vendor: the sources' `Signer`, matched against the certificate's `O=`. The file is held open with
share = Read from before the check until the installer exits. Each CUDA archive is held the same way
from before its hash until `tar` is done, because `.downloads` may be writable by other accounts.

**The batch is generated code.** Every path, version and URL enters it through `Lit`, a
single-quoted literal escaped by PowerShell's `CodeGeneration.EscapeSingleQuotedStringContent`. That
also covers the typographic quotes the parser accepts as a closing quote.

**Markers.**

- `.cuda-version` is written once every component is in, and a tree without it is cleared and
  redone. The version is the directory name, since the assembled tree has no `version.json`.
- `.therock-version` follows a successful extract, and the environment step is gated on it.
- `.vulkan-version` needs `Lib\vulkan-1.lib` and `Bin\glslc.exe` left by the installer.

**Vulkan install.** The download goes to `<DistRoot>\VulkanSDK\.downloads`, and the installer runs
with `--root <dir> --accept-licenses --default-answer --confirm-command install`. The loader step
copies `vulkan-1.dll` out of System32, where the SDK's runtime installer `Helpers\VulkanRT.exe`
leaves it (LunarG deleted the standalone runtime archives, for every version), and checks it
(Authenticode, `O=LunarG, Inc.`) inside the locked dist before it takes its name: a driver's own
loader, signed by the vendor, is not copied. A System32 with no LunarG-signed loader leaves no
`Runtime\vulkan-1.dll`, and `xtask package` then refuses; the report names running that installer
as the step to take.

**sccache install.** It runs outside the elevated session. The newest `x86_64` or `aarch64`
`pc-windows-msvc` zip is staged and moved over the target, so an exe in use stays intact and the run
says so. The user PATH goes through the raw registry with a `WM_SETTINGCHANGE` broadcast. A leftover
winget `Mozilla.sccache` is uninstalled once the release copy works.

**`scripts\dists.ps1` is PS 5.1-safe and ASCII-only**, because the script that dot-sources it runs
under Windows PowerShell 5.1.

</details>

### Linux prerequisites

`scripts/00-install-prerequisites.sh` installs what xtask cannot build without, then runs
`xtask prereq`.

**The apt packages are the one step that needs root.** They go through `sudo`, only when one is
missing, and without a terminal `sudo -n` fails in one line instead of hanging. On a system without
`apt-get`, the script lists them for you to install by hand.

| Packages | Why |
| --- | --- |
| `git cmake ninja-build make pkg-config curl jq xz-utils ca-certificates` | The build and the downloads |
| `g++` | clang takes libstdc++, the start files and libgcc from the distro GCC, and nvcc drives g++ as its host compiler |
| `libssl-dev` | HTTPS in llama-server |
| `nodejs npm` | The web UI in `fe/` and the chat UI llama-server embeds |
| `unzip` | The NCCL wheel |
| `libfontconfig-dev` | The configurator's Skia renderer |
| `patchelf dpkg-dev rpm` | `xtask package`: the RUNPATH rewrite, the `Depends` computation and `rpmbuild` |

The rest runs in user scope, and `--skip apt,brew,rust,sccache,rocm,cuda,nccl,vulkan` skips steps.
The roots must be writable by the current user.

| Step | Source | Notes |
| --- | --- | --- |
| Rust | `rustup` (`--profile minimal`, stable, `--no-modify-path`) | Only when no `cargo` is found |
| sccache | The `unknown-linux-musl` tarball | Moved over `~/.local/bin/sccache` |
| ROCm | `therock-dist-linux-multiarch-<X.Y.Z>.tar.gz` | ~9.5 GB download, ~22 GB extracted; needs 45 GiB free |
| CUDA | The `linux-x86_64` entries of the newest manifest | Needs 8 GiB free; the version directory is the full `X.Y.Z` |
| NCCL (optional) | The PyPI wheel for the installed toolkit's major | ~290 MB download, ~345 MB of `libnccl.so.2`; needs 2 GiB free |
| Vulkan SDK | `vulkansdk-linux-x86_64-<version>.tar.xz` | Needs 4 GiB free |

**Downloads go to `<dist root>/<Name>/.downloads` and resume.** A finished archive is deleted after
its install. A version directory without its marker is an interrupted install of that version, and
it is cleared and redone.

**CUDA needs a `lib64` link.** The archives put libraries in `lib/`, while nvcc links its own
runtime (`cudart_static`, `cudadevrt`) from `<toolkit>/lib64`. The install adds `lib64 -> lib`, and
the completeness test goes through it.

**NCCL comes from PyPI because NVIDIA keeps it out of the toolkit redist** and publishes no archive
index for it. The wheel needs no root and no distro match. Only its `nvidia/nccl/{lib,include}` is
kept, with `libnccl.so` linked to the soname `libnccl.so.2`, since the wheel has no link name. The
marker records the CUDA major, and a dist for another major does not count as installed. A failed
install is reported but is not a gap: the build goes on without it.
