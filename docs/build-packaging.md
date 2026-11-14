---
title: Packaging and install
type: reference
updated: 2026-11-14
---

# Packaging and install

What `xtask package` puts in the two MSIs and in each `.deb` and `.rpm`, and how `xtask install` lays
the packages down. The `.pkg` files are on [the macOS page](build-macos.md). The MSIs' own authoring is in
[installer\AGENTS.md](../installer/AGENTS.md), and the phases and what breaks are in
[Build pipeline](build-pipeline.md).

## Packaging

**What a machine installs is self-contained: the target needs the GPU drivers and little else.** On
Windows that also means the VC++ runtime. Each OS stages into `build/<os>/staging/`, and the staged
tree is the installed tree.

**Windows ships two MSIs, Linux eight packages per format and macOS five, so a machine can leave out
the runtime of a GPU it does not have (Linux), and a headless server the GUI.** On Linux and Windows
the engine, llama.cpp, is packaged apart from the framework, under its own name and version
(`PLAN.md` step 11, R12): on Windows llama.cpp's own MSI holds the engine and all its GPU runtimes,
and the framework's MSI requires it. On macOS too (type-checked only, never run on a Mac) the engine and
its Vulkan runtime install into `/opt/llama.cpp`, the framework into `/opt/private-ai`. On Linux and macOS
`xtask/src/common/components.rs` splits the staged tree by path, so no file lands in two packages or
in none; on Windows each product has its own staging tree. The engine's configuration follows: the
instance files are in `/etc/llama.cpp/` on Linux and `%ProgramData%\llama.cpp\config\` on Windows,
and per user on macOS.

| OS | Package | File in `dist/` |
| --- | --- | --- |
| Windows | The framework's MSI | `private-ai-framework-v<framework>-x64.msi` (`arm64` on an ARM build) |
| Windows | llama.cpp's MSI, with its GPU runtimes | `llama.cpp-v<release>-x64.msi` (no `+pai<N>`; `arm64` on an ARM build) |
| Linux | The framework: `config`, `cli`, `bff`, `core` | `private-ai-framework-<package>-v<framework>-amd64.deb`, `-x86_64.rpm` (`arm64`, `aarch64` on ARM) |
| Linux | The engine, `llama.cpp` | `llama.cpp-v<release>+pai<N>-amd64.deb`, `-x86_64.rpm` (the same on ARM) |
| Linux | Its runtimes, `llama.cpp-rocm`, `-cuda`, `-vulkan` | `llama.cpp-<sdk>-v<sdk version>-amd64.deb`, `-x86_64.rpm` (the same on ARM) |
| macOS | The engine `llama.cpp` and `llama.cpp-vulkan`; the framework's `config`, `cli`, `core` | `llama.cpp-v<release>-arm64.pkg`, `llama.cpp-vulkan-v<sdk version>-arm64.pkg`, `private-ai-framework-<package>-v<framework>-arm64.pkg`: [the macOS page](build-macos.md) |

For example, `private-ai-framework-core-v2.99.1-amd64.deb`, `llama.cpp-v1.0.0+pai1-amd64.deb`,
`llama.cpp-rocm-v10.1.0-amd64.deb`, `private-ai-framework-v2.99.1-x64.msi`, `llama.cpp-v1.0.0-x64.msi`. Release 2.4 and
earlier named them `llama-cpp-framework-*`: the new packages upgrade those in place (below, per format).
A [minimal build](build-compiling.md#the-minimal-build) (Windows and macOS, CI only) names the engine's
file `llama.cpp-v<release>-minimal-<arch>.msi` or `.pkg` and is never a release asset.

- `<framework>` is the `[workspace.package]` version of the root `Cargo.toml`, one for the
  `pai-*` crates: the configurator and the framework are versioned together.
- `<release>` is `git describe --tags` of the clone, which prefers the annotated release tag over
  the nightly on the same commit. On every OS `xtask package` refuses a clone that is not on an
  exact `vX.Y.Z` release (on Windows also one beyond an MSI ProductVersion, 255.255.65535).
- `<N>` is our build revision of that release, `LLAMA_CPP_REVISION` in `components.rs` (1), raised
  for any change that is not llama.cpp's code on the same release (the patches, the build options,
  the package's scripts or files) and back to 1 at every new llama.cpp release (user, 2026-11-14).
- `<sdk version>` is the version of the SDK in the dist layout. An SDK outside the layout takes the
  framework's version.

**Every OS ships the benchmark method read-only** in `share/benchmarks/`: each `standard-<X.Y>.ini`,
`standard.versions`, and the prompts with their images. `bench_share.rs` stages it, and on Linux and
macOS it is in the `cli` package.

**Every OS ships the configurator's text read-only** in `share/strings/`: `en_US.toml`, the GUI's
language file, and `cli.toml`, the text every program prints (`pai`, `pai-manager`, `pai-bff`).
`strings_share.rs` stages every `*.toml` of `crates/core/strings/`; on Linux and macOS `cli.toml` is in
the `core` package (on Linux every other framework package requires it; a `.pkg` requires nothing),
and the GUI's language files are in `config`.

**llama.cpp's test harnesses are pruned on every OS.** On Linux every staged executable lands in
`/usr/bin`, and on macOS each gets a command link, so shipping them would drop generically named
`test-*` commands into every shell; on Windows they are unit tests no installed engine needs. cmake stages every
executable with an `install()` rule, so count them from the staging tree, not the build directory.

**Which runtime libraries is never a list.** It is the closure of what the staged binaries link,
restricted to what resolves inside a dist. Everything else stays a system library.

**Every package carries its licences.** Each MSI holds its own in `share\licenses\` beside `bin\`. On Linux
each package holds its own `/usr/share/private-ai/licenses/<package>/`, and on macOS its own
`share/licenses/<package>/` in its prefix (`/opt/llama.cpp` or `/opt/private-ai`), since no two packages can own one file (the directory is
named by the component: `llama/` for the `llama.cpp` package). `licenses.rs` (`xtask/src/common/`) stages them:

- This project's licence, llama.cpp's and what llama.cpp's own `licenses/` holds, the configurator's
  icon font, and a text for every bundled library.
- `README.txt` says which text covers which library. llama.cpp's own libraries are covered by its
  licence.
- **The table in `licenses.rs` decides which licence a library has; nothing is guessed.** The package
  is refused when a bundled library (any `.dll`, `.dylib`, `.so` or `.so.*` under the staged tree) is
  not named there, or when its text cannot be found. A new library in a dist is read before it ships.
- A text comes from the dist when it has one, so it matches the bundled version: ROCm's
  `share\doc\<name>\LICENSE*`, CUDA's `LICENSE`, OpenSSL's `license.txt`. Otherwise it comes from
  `installer/licenses/`, whose README says where each was taken from: the Vulkan loader is Apache
  2.0 there on every OS, since it is Khronos' open-source loader code, whatever package carries it.
- NVIDIA's terms are not an open-source licence. They install as `NVIDIA-CUDA-EULA.txt`, and
  `ggml-cuda` needs them too, since it links NVIDIA's static runtime: on Linux both `llama.cpp` and
  `llama.cpp-cuda` carry them.

### The MSIs (Windows)

**Windows ships two installers: llama.cpp's own MSI and the framework's, which requires it.** Neither
has been compiled by WiX or run on Windows since the split (commit `836fdc7`), nor since the
instances are kept across an engine upgrade (`9462eab`): the `.wxs` files are checked as XML by
xtask's tests, their custom-action conditions evaluated over install, repair, upgrade, uninstall and
downgrade, and the Rust is type-checked only. The authoring, the services, the
ACLs and the test plan are in [installer\AGENTS.md](../installer/AGENTS.md).

| | llama.cpp's MSI | The framework's MSI |
| --- | --- | --- |
| Holds | llama.cpp's executables and DLLs, the ggml backends, the GPU runtimes and OpenSSL, the read-only defaults | `pai-config.exe`, `pai.exe`, `pai-manager.exe`, `pai-bff.exe`, the benchmark method, the strings |
| Version | The llama.cpp release | The framework version |
| Installs to | `%ProgramFiles%\llama.cpp\` (choosable), registered in `HKLM\Software\llama.cpp` | `%ProgramFiles%\private-ai\` (choosable), registered in `HKLM\Software\private-ai` |
| Machine data | `%ProgramData%\llama.cpp\{config,logs,models,state}` | `%ProgramData%\private-ai\config\` (`bff.ini`) |
| Services | None; stops and restarts the running instances around a repair or upgrade, deletes the instance services on uninstall | `private-ai-manager`, `private-ai-bff` |
| Options | None | PATH entry for `bin\`, Start Menu shortcut |

**`xtask package [llama|framework]` checks WiX first, then stages, builds into `dist/` and validates
each product** (default: both, llama.cpp's first). The check costs seconds, not a staging run: tool
found, EULA accepted, extensions present (added when missing). Both need the llama.cpp clone on a
release: the framework's MSI records that release as the oldest engine it installs over.

- WiX resolves as `--wix <wix.exe>`, then the recorded `WixExe`, then PATH and the dotnet tools
  directory.
- A non-pinned version gets a warning. WiX 6.0.2 built the single MSI before the split without the
  v7 EULA.
- `wix msi validate` runs the ICEs on each, with ICE61 suppressed.
- It refuses a package of 2 GiB or more, GitHub's asset limit.
- Measured on the single MSI before the split, not repeated: on the 2.8 GB staging tree, a 1.16 GiB
  MSI in 2 to 3.5 minutes of `wix build`, plus about 40 s of validation. Nearly all of it is
  llama.cpp's MSI's now.

**Staging** puts each product in its own tree, `build\windows\staging\llama\` and
`staging\framework\`, which is exactly what its MSI installs, so an administrative image (`msiexec
/a`) compares 1:1 with it. llama.cpp's uses `cmake --install`, then removes `test*.exe` and
`include\`, which never shipped, and writes the defaults into `share\defaults\`. The framework's gets
`pai-config.exe`, `pai.exe`, `pai-manager.exe` and `pai-bff.exe` from cargo's release output
(`msi::CARGO_EXES`) and `share\benchmarks\` and `share\strings\`. Both get `llama.ico` in `bin\`, and
a missing icon is regenerated here. Filtering happens in staging and not in the `.wxs`, which harvests
each tree whole, but for `pai-manager.exe` and `pai-bff.exe`, each the key path of the component that
installs its service.

| Bundled into llama.cpp's `bin\` | How |
| --- | --- |
| OpenSSL `libcrypto-N-x64.dll`, `libssl-N-x64.dll` | Names scanned from every staged binary, copied from `OpenSSLDir\bin` |
| GPU runtimes | Import closure, restricted to the ROCm dist's `bin\`, the CUDA dist's `bin\x64` or `bin`, the Vulkan SDK's `Bin\`, and its `Runtime\` for `vulkan-1.dll` |
| `amd_comgr*.dll` | By name from the ROCm dist: no import table shows it |
| `rocblas\library`, `hipblaslt\library` (`.kpack\blas_lib_*` beside `bin\`) | For the architectures in `GpuTargets` only, plus whatever names no architecture |

The GPU runtimes add about 1.6 GB on top of the binaries. It refuses a `ggml-hip.dll` without
`hipblas.dll`, a `ggml-cuda.dll` without `cublas64_*.dll`, or a `ggml-vulkan.dll` without
`vulkan-1.dll`, since ggml would silently drop that backend on every other machine.

**The Vulkan loader is bundled, because a driver does not guarantee one.** A machine can have the
GPU driver and no `vulkan-1.dll` in System32. The SDK ships no loader of its own, only the import
lib and the runtime installer `Helpers\VulkanRT.exe`, and LunarG no longer publishes the standalone
runtime archive the SDK's version used to ship, so the prerequisites script copies the DLL out of
System32, where that installer leaves it, into `<DistRoot>\VulkanSDK\<version>\windows\Runtime\`.
The DLL must pass Authenticode (`O=LunarG, Inc.`): a driver's own loader, signed by the vendor, is
not bundled, and an SDK already installed gets it on the next run. Staging copies the DLL through
the import closure, and its licence is the vendored Apache 2.0 of the loader's own code, the same
`vulkan-loader.txt` Linux's `libvulkan.so.1` carries.

**Each MSI shows a licence page** (WixUI `LicenseAgreementDlg`, with the accept checkbox) between the
welcome page and `CustomizeDlg`. `xtask package` writes its text beside the product's staging tree
(`build\windows\staging\<product>.license.rtf`, not installed): which text covers what, the MIT
licence, and, in llama.cpp's, NVIDIA's terms in full. A silent or basic-UI install does not show it.

**The framework's MSI refuses to install without llama.cpp at the release it was built against or
newer.** Windows Installer conditions compare no versions, so it finds the engine by llama.cpp's
UpgradeCode with a minimum version and fails a launch condition without it. Each MSI upgrades on its
own (`MajorUpgrade`, same-version repackages allowed, downgrades refused): a newer llama.cpp needs no
framework rebuild.

**llama.cpp's MSI ships its defaults and copies them into `%ProgramData%\llama.cpp\config\` only
when missing**, never over a file, on install, repair and upgrade alike, and leaves them on uninstall
(`NeverOverwrite`, `Permanent`). They are `crates/core/defaults/windows/`: a `server@default.ini`
holding only `[Server]` `Port = 9931` and `Hostname = localhost`, and an empty `presets.ini`. As on
Linux the manager reads the server file with pai's defaults for every other key and rewrites it in
pai's template at its first write. The instance `default` therefore exists from the engine's install.

**The framework's MSI installs the `private-ai-manager` service.** It is `pai-manager.exe serve` as
LocalSystem, started at boot and restarted after a failure; it runs each instance as its own
service, `llama.cpp_<id>`, under the virtual account `NT SERVICE\llama.cpp_<id>` (created with the
manual start type; automatic once the configurator's "start llama-server when the system starts" is
on, and the SCM then starts it at boot: the manager starts none itself), and serves the configurator
over `\\.\pipe\private-ai-manager`. The instance runs the llama-server of llama.cpp's install, found
through `HKLM\Software\llama.cpp` `InstallDir` (a tree build's own wins). `%ProgramData%\llama.cpp\`
holds `config\` (the instances' server and preset files, which users reach only through the
manager), `logs\` (each instance's `llama-server@<id>.log`), `models\` and `state\`, each folder with a
protected DACL. An instance whose `server@<id>.ini` leaves `ModelsDir` or `StateDir` unset uses
`models\` or `state\` there, not the profile of the account it runs under, and runs in `state\`; `pai`
and the configurator's Server tab leave both unset at a save (a Hugging Face download still pins
the user's `~\.llama.cpp`: the known gaps in [installer\AGENTS.md](../installer/AGENTS.md)). A removal, a repair or the old half of an upgrade of the framework runs `pai-manager
uninstall-services` to delete the instances' services, which the manager creates at run time and
Windows Installer does not know; the new manager recreates them.

**The framework's MSI installs the web service `private-ai-bff`:** `pai-bff.exe service`, started at
boot, as the virtual account `NT SERVICE\private-ai-bff`, restarted 5 s after a failure. The SCM stops
it before its files are replaced or removed and deletes it on uninstall; the process stopper leaves
it alone. It reads `%ProgramData%\private-ai\config\bff.ini` through the read `NT SERVICE\ALL
SERVICES` has on that folder.

**llama.cpp's MSI keeps the running instances across a repair and an upgrade.** Before its files are
replaced (the old package, in an upgrade) a script writes the running `llama.cpp_*` services to
`%ProgramData%\llama.cpp\instances.running` and stops them; once the new files are in place another
starts those again and deletes the file. A stopped instance stays stopped.

**Uninstalling llama.cpp's MSI deletes the instance services and asks about its configuration.** A
script deletes every `llama.cpp_*` service on an uninstall, not on an upgrade, a repair or with
`KEEPINSTANCES=1`, which only `xtask install`'s downgrade passes; the engine does not require the
framework, so the framework's manager may be absent. The removal page offers to delete
`%ProgramData%\llama.cpp\` with a checkbox, cleared by default; a silent or basic-UI uninstall, and
every upgrade, keeps it.

**Release 2.4 is removed by llama.cpp's MSI:** 2.4 (`llama-cpp-framework`, the framework's
UpgradeCode) installed into `Program Files\llama.cpp\` and registered `HKLM\Software\llama.cpp`, both
the engine's now, so llama.cpp's MSI removes a framework install below 2.99.0 first. The framework's
MSI no longer carries over a 2.4 install folder: it installs into `Program Files\private-ai\` unless a
2.99+ install recorded another. Its guard against a non-MSI install checks the `Uninstall\llama.cpp`
and `Uninstall\private-ai` entries, which llama.cpp's MSI-written entry passes (llama.cpp's MSI has
the same guard on `Uninstall\llama.cpp`, which 2.4's MSI-written entry passes), and `StopProcesses`
reads only `HKLM\Software\private-ai`. Details and the test step:
[installer\AGENTS.md](../installer/AGENTS.md).

**System libraries stay external:** the VC++ redist x64 (`libomp140.x86_64.dll` included) and the
GPU drivers. cudart is static. The load-order reasons are in [installer\AGENTS.md](../installer/AGENTS.md).

<details>
<summary>Why the OpenSSL scan works the way it does</summary>

The DLLs are normal imports. Without them, the server fails at load on a machine with no OpenSSL.
The app directory wins the DLL search order, so they go in `bin\`.

- Names are scanned from the binaries, because the ABI number moves.
- Every staged `.exe` and `.dll` is scanned, never a name pattern. llama.cpp tools are thin
  launchers plus impl DLLs: `llama-server.exe` imports `llama-server-impl.dll`, which carries the
  OpenSSL imports. A name-based scan breaks on the next rename.
- Do not copy `*.dll` wholesale. Most of `OpenSSLDir\bin` is providers: `legacy`, `p_test`,
  `p_ossltest`, `p_minimal`.
- The scan reads 4 MB chunks with a 64-byte overlap, because `ggml-hip.dll` is about 1 GB. A search
  for the prefix gates the match.
- No OpenSSL import found in any binary fails the package, since the scan or the link has broken.

</details>

### The `.deb` and `.rpm` (Linux)

**`xtask package` builds eight packages per format from one staged tree, about 3.8 GiB installed with all
of them.** `--deb` or `--rpm` builds one format. `xtask package llama` packages the engine and its three
runtimes alone, `xtask package framework` the framework's four alone; the other product's files in
`dist/` are left as they are (run on this machine on 2026-11-14: `package llama --deb` rewrote only the
four engine `.deb`, `package framework --deb` only the four framework ones, `package` all sixteen). It
checks first what the product needs and refuses without it: the engine `llama-server` in
`llama.cpp-cmake/bin/`, the framework `pai-config`, `pai`, `pai-manager` and `pai-bff` in
`cargo-target/release/`. It also refuses, for the framework, a built `pai-config --version` that
differs from the root `Cargo.toml`'s (a bump without a rebuild), and, for either, a llama.cpp clone not
on a `vX.Y.Z` release tag: the framework needs the clone only for that tag, the minimum engine version
it requires.

**The framework's executables are staged from `cargo-target/release/`, only stripped.** They link
nothing of llama.cpp, so they get no RUNPATH and no library closure; the copies of `pai-config` and `pai`
beside `llama-server` are the build tree's, not the packages'. Checked on 2026-11-14: the `-cli`
`.deb`'s `pai` runs with no RUNPATH and no missing library. llama.cpp's own files are stripped and get the
RUNPATH described below.

| Package | Holds | Version | `.deb` / `.rpm` |
| --- | --- | --- | --- |
| `private-ai-framework-config` | `pai-config` (the GUI), its language files, the desktop entry and its icon | `<framework>` | not measured |
| `private-ai-framework-cli` | `pai` (the command line), the benchmark method; no graphics library | `<framework>` | not measured |
| `private-ai-framework-bff` | `pai-bff` (the web service), `private-ai-bff.service` and the sysusers entry of its user | `<framework>` | not measured |
| `private-ai-framework-core` | `pai-manager`, `private-ai-manager.service`, the D-Bus and polkit policies, the sysusers entry of `private-ai`, `cli.toml`, the manager's maintainer scripts | `<framework>` | not measured |
| `llama.cpp` | llama.cpp's executables and libraries, the ggml backends (`libggml-hip.so`, `libggml-cuda.so`, `libggml-vulkan.so`), libomp, the instance unit `llama.cpp@.service` and the scripts that stop and restart the instances; nothing else of the framework | `1:<release>+pai<N>`, `1:1.0.0+pai1` | not measured |
| `llama.cpp-rocm` | The ROCm libraries with RCCL, and the kernel stores | The SDK's, `10.1.0` | 831 / 922 MiB |
| `llama.cpp-cuda` | The CUDA libraries and NCCL | `<cuda>+nccl.<nccl>`, `13.5.0+nccl.2.32.3` | 655 / 675 MiB |
| `llama.cpp-vulkan` | The Vulkan loader | The SDK's, `1.4.363.1` | 2 / 2 MiB |

The runtimes' sizes are of the v1.0.0 build (framework 2.4.0), as `xtask package` printed them, rounded
up to the MiB: they hold the same files as then. The framework's packages and `llama.cpp` divide 2.4's
`config` (18 / 21 MiB) and `llama` (190 / 229 MiB) another way and have not been measured.

**The engine carries its own version, the framework packages only the framework's.** `llama.cpp` is
`1:<release>+pai<N>`: the epoch `1:` because Ubuntu 26.04 ships its own `llama.cpp` at `8681+dfsg-1`,
which apt would otherwise rank above `1.0.0` (the epoch is compared first), and `+pai<N>` our build
revision. The `.rpm` spells it Epoch `1`, Version `<release>`, Release `pai<N>`; every other `.rpm` has
Release `1` and no epoch. A newer llama.cpp therefore installs without a framework rebuild.
`xtask install` compares the epochs and reads an rpm's version with `%{EVR}`.

**A runtime package holds only the external SDK's libraries, so it changes only with the SDK.** A new
llama.cpp on the same SDKs leaves the three as they are. Each is optional: a backend whose runtime is
missing is skipped at start, with only a log line.

**The relations:**

| Relation | `.deb` | `.rpm` |
| --- | --- | --- |
| `config` needs `cli` and `core`; `cli` and `bff` need `core`; each at exactly its version | `Depends` `(= <version>)` | `Requires` `= <version>` |
| `config`, `cli` and `core` need the engine at least at the release they were built against; `bff` does not run it | `Depends: llama.cpp (>= 1:<release>)` | `Requires: llama.cpp >= 1:<release>` |
| `llama.cpp` asks for each runtime at the version it was built against | `Recommends` `(= <version>)` | `Recommends` `= <version>` |
| `llama.cpp` refuses any other version of a runtime, whose symbols a backend may not resolve | `Breaks` `(<< <version>)`, `(>> <version>)` | `Conflicts` `< <version>`, `> <version>` |
| `llama.cpp` keeps out the distribution's package of the same commands | `Conflicts: llama.cpp-tools` (Ubuntu) | `Conflicts: llama-cpp` (Fedora) |
| `config`, `llama.cpp` and the runtimes replace `llama-cpp-framework`, the single package of the releases up to 2.2.0 | `Conflicts`, `Replaces` | `Obsoletes` `<= 2.2.0` |
| The same replace their component's package of releases 2.3 and 2.4 (`llama-cpp-framework-config`, `-llama`, `-rocm`, `-cuda`, `-vulkan`) and stand in for it; `cli`, `bff` and `core` had none | `Conflicts`, `Replaces`, `Provides` `(= <version>)` | `Obsoletes` `< 2.4.1` (`config`, `llama.cpp`) or `<= <version>` (a runtime), `Provides` `= <EVR>` |

**The `Obsoletes` is versioned, the `.deb` pair is not.** rpm matches an unversioned `Obsoletes` against
every version of the name, future ones included, so it would remove a package that later takes the name
back: rpmbuild warns about it. dpkg warns about no such thing and its `Replaces` is only about taking over
the old files, so those stay unversioned. Against the 2.4 component packages, `< 2.4.1` catches every `2.4.0+llama.cpp.<tag>`
(rpm orders it above 2.4.0, below 2.4.1); a runtime is versioned by its SDK, with no last version known
under the old name, so its own version bounds the old package.

**Checked on a real install** (Ubuntu 26.04, 2026-11-14, the `.deb` files of 2.99.1 with llama.cpp
`1:1.0.0+pai1`, reported by the user): the eight packages installed over 2.4.0 and removed its five
packages; the users `private-ai` and `private-ai-bff` were created, `private-ai-manager` and
`private-ai-bff` were active and no `llama-server` was started; `pai --version` printed `2.99.1`; polkit
refused `nobody` the manager's `Stop`, `WriteFile` and `ReadFile` of `server@default.ini` with
`NotAuthorized`; the bff answered `/api/version`, `/api/instances` with `[]` and 200, and 503
`unavailable` with the manager stopped. The `.rpm` files have not been installed.

On the same install, the same day, with the start at boot of step 12 (reported by the user):
`pai server boot on` left `systemctl is-enabled llama.cpp@default` at `enabled` and created the link in
`multi-user.target.wants/`, `pai server boot off` left it `disabled`, and the Settings toggle matched
systemd both ways. Reinstalling at the same version after `llama.cpp@.service` moved from `core` to
`llama.cpp` needed a one-off `dpkg --force-overwrite`, since the installed `core` of that version still owned the
file. Only pre-release builds of one version hit it; no fix is planned. The stop and restart of the services by the scripts (below) were checked in
xtask's tests with a fake `systemctl`, then once on this install, the same day, by an upgrade to the
packages that move the instance files to `/etc/llama.cpp/` (reported by the user; the user's files
had been copied there by hand beforehand): it stopped `llama.cpp@default` (14:25:15), replaced the
files and started it again (14:25:21), reading `/etc/llama.cpp/presets.ini`; `/run/llama.cpp.instances`
was gone afterwards, the shipped defaults had not overwritten the user's files, and the bff listed
`default` active. A first install with no file in `/etc/llama.cpp/`, a removal and a purge have not
been run for real.

| Path | Package | Holds |
| --- | --- | --- |
| `/usr/bin/` | `llama.cpp`; `pai-config` in `config`, `pai` in `cli` | llama.cpp's executables, `pai-config` and `pai`, the real files |
| `/usr/libexec/private-ai/` | `core`; `pai-bff` in `bff` | `pai-manager` and `pai-bff`, which the units call by full path |
| `/usr/lib/private-ai/` | `llama.cpp` | llama.cpp's libraries (`libllama`, `libmtmd`) and the ggml backends, a private directory, never a system library directory |
| `/usr/lib/private-ai/lib/` | The runtimes; `llvm/lib/libomp.so` in `llama.cpp` | The bundled runtimes |
| `/usr/lib/private-ai/.kpack/` | `llama.cpp-rocm` | hipBLASLt's `blas_lib_*` and RCCL's `rccl_lib_*` archives; ROCm looks in `<libdir>/../.kpack`, so they cannot sit at the top |
| `/usr/share/private-ai/benchmarks/` | `cli` | The benchmark method |
| `/usr/share/private-ai/strings/` | `config`; `cli.toml` in `core` | The GUI's `en_US.toml`, and `cli.toml`, the text every program prints |
| `/usr/share/private-ai/llama.cpp/` | `llama.cpp` | The defaults `server@default.ini` and an empty `presets.ini`, copied into `/etc/llama.cpp/` when missing |
| `/usr/share/private-ai/licenses/<component>/` | Each its own (`llama/` for `llama.cpp`) | The licence texts and their `README.txt` |
| `/usr/share/` | `config`; `doc/<package>/copyright` in each | The desktop entry `org.privateai.Config.desktop` ("Private AI Framework" in the menu), its icon, the copyright file |
| `/usr/lib/systemd/system/`, `/usr/lib/sysusers.d/`, `/usr/share/dbus-1/system.d/`, `/usr/share/polkit-1/actions/` | `core`; `llama.cpp@.service` in `llama.cpp`; `private-ai-bff.service` and `private-ai-bff.conf` in `bff` | The services' system files, below |

**Each package's `copyright` and `Homepage` (rpm `URL`) name its own source.** The framework's
packages carry this project's MIT licence and repository; `llama.cpp` carries llama.cpp's own licence
and its upstream, `https://github.com/ggml-org/llama.cpp`; a runtime's describes the SDK libraries it
redistributes, with the same upstream.

**Staging skips `cmake --install` and places each file of the build's `bin/` by name** (`place` in
`xtask/src/linux/package.rs`): a `lib*.so*` in `/usr/lib/private-ai`, every other executable in
`/usr/bin`, library symlinks kept as symlinks. `pai-manager` and `pai-bff` come from cargo's target dir.
`paths::llama_bin` finds `llama-server` beside `pai-config` and `pai` in `/usr/bin`, and from `pai-manager` in
`/usr/libexec/private-ai` through `../../bin`; `paths::installed_share_dir` finds the data in
`/usr/share/private-ai` from `/usr/bin` and from `/usr/libexec/private-ai` alike
(`paths::installed_share_candidates`), so the services print texts, not bare string keys. The rpm `%files` owns no system directory, and the private libraries stay out
of `Provides`.

**ggml finds its backends through `GGML_BACKEND_DIR=/usr/lib/private-ai`** (`xtask/cmake/linux.toml`),
which ggml searches before the executable's directory and the working directory
(`ggml_backend_load_best` in `ggml/src/ggml-backend-reg.cpp`). So while the `llama.cpp` package is installed,
a `llama-server` built in the tree loads the installed backends, not its own; with nothing installed
there, it finds its own beside it.

**The runtimes are the `ldd` closure of the staged files**, restricted to the ROCm, CUDA and NCCL
dists and the Vulkan SDK's loader directory. Each lands under its soname with symlinks dereferenced.

- `libcuda.so.1` is the NVIDIA driver's and stays out.
- If the system loader answered for Vulkan, the SDK's `libvulkan.so.1` is taken by name.
- A `libggml-hip.so` without `libhipblas.so.*`, or a `libggml-cuda.so` without `libcublas.so.*`, is
  refused.
- `libamd_comgr.so.3` is a plain `DT_NEEDED` of `libamdhip64.so.7`, so the closure carries it.
- `librccl.so.1` and its dependencies arrive through the same closure.
- A RUNPATH precedes the system library directories, so no system ROCm can get mixed in, short of
  `LD_LIBRARY_PATH`.

**The ROCm part mirrors the dist's `lib/` layout** (`lib/`, `lib/llvm/lib/`,
`lib/rocm_sysdeps/lib/`, `lib/rocblas/library/`, `lib/hipblaslt/library/`). AMD's libraries find
each other and their kernel files through `$ORIGIN`-relative RUNPATHs and paths, so a mirrored tree
needs no patching of anything AMD shipped. `libomp.so` comes along in the `llama.cpp` package, not `llama.cpp-rocm`,
because ggml-base itself links the dist's OpenMP runtime.

CUDA (`libcudart`, `libcublas`, `libcublasLt`, about 575 MiB), NCCL and the Vulkan loader go flat
into `lib/`, in the `cuda` and `vulkan` packages.

**Kernel data, which `ldd` cannot show, is copied only for `gpu_targets`,** files or per-arch
directories alike, plus whatever names no architecture.

| Store | Packaged | Whole dist |
| --- | --- | --- |
| `lib/rocblas/library` | 359 MiB | 810 MiB |
| `lib/hipblaslt/library` | 467 MiB | 2198 MiB |
| `.kpack` | 91 MiB | 2258 MiB |

**RUNPATH is rewritten with `patchelf` on llama.cpp's ELF files staged in `/usr/bin` and
`/usr/lib/private-ai` only** (the runtimes keep AMD's, the framework's executables get none), after `strip --strip-unneeded` and never before, since stripping a
patched file can drop the section patchelf appended. The value is absolute,
`/usr/lib/private-ai:/usr/lib/private-ai/lib:/usr/lib/private-ai/lib/llvm/lib`. `LD_LIBRARY_PATH`
still wins over RUNPATH. That no file is opened under the dists or the build tree was checked by hand
(extracting the `.deb`, `strace` of a generation per backend, not recorded) with a `$ORIGIN`-relative
RUNPATH; it has not been repeated with the absolute one.

**No config file is shipped; `llama.cpp` ships its defaults.** The server and preset files
(`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`) are the engine's, in `/etc/llama.cpp/`
(`paths::engine_config_dir`), which only the manager writes; the framework's own (`bff.ini`) are in
`/etc/private-ai/`. systemd creates both at the manager's start (`ConfigurationDirectory=`).
`llama.cpp` holds, read-only in `/usr/share/private-ai/llama.cpp/`, `server@default.ini` and an
empty `presets.ini`. The server file holds starting values only, a `[Server]` section with `Port`,
`Hostname`, `ModelsDir` and `StateDir` (the service user's paths under `/var/lib/private-ai`): the
format belongs to the framework, so a change of pai's template does not touch the engine package.
The manager reads it with pai's defaults for every other key and rewrites it in pai's full template
at its first write (`server_cfg.rs` tests, which also check that the manager accepts it). Its scripts copy each into `/etc/llama.cpp/` only
when it is missing, never over a file, so an upgrade leaves what the user or the manager wrote and
dpkg asks nothing: `server@default.ini` 0640 `root:private-ai` when that group exists, else 0644,
`presets.ini` 0644. The instance `default` therefore exists from the first install. No file is moved
from `/etc/private-ai/`: there is no migration between the pre-release layouts. `pai-config` keeps `settings.ini` per
user in `${XDG_CONFIG_HOME:-~/.config}/private-ai/` (`paths::config_dir`) and moves release 2.4's
`~/.config/llama.cpp/` and `~/.local/share/llama.cpp/` there at its first start
([Migration from 2.4](../crates/config/README.md#migration-from-24)).

**The services' system files are in `core`, beside `pai-manager`, the instance unit in `llama.cpp`, and the web service's unit and user in `bff`** (`xtask/src/linux/system_files.rs`,
installed 0644, overwritten on every upgrade; local changes go in a drop-in, `systemctl edit`):

| File | What it does |
| --- | --- |
| `/usr/lib/systemd/system/private-ai-manager.service` | `pai-manager serve` as root, `Type=dbus` on `org.privateai.Manager1`, `HOME=/var/lib/private-ai` so a blank `ModelsDir` means the same as for the instances, `ConfigurationDirectory=private-ai llama.cpp` (mode 0755), `Restart=on-failure`, enabled for `multi-user.target` |
| `/usr/lib/systemd/system/llama.cpp@.service` | `pai-manager run-instance %i` as user `private-ai` (supplementary groups `render`, `video`), `StateDirectory=` for its home `/var/lib/private-ai` and the data root under it, `Restart=on-abnormal`, `ConditionFileIsExecutable=/usr/libexec/private-ai/pai-manager` (without the framework it does not run: the engine does not require it); `[Install] WantedBy=multi-user.target`, so an instance starts at boot once enabled, which no package does (the configurator's "start llama-server when the system starts", through the manager's `SetBootEnabled`); its output goes to journald |
| `/usr/share/dbus-1/system.d/org.privateai.Manager1.conf` | Only root owns the name; anyone may call it, and polkit decides in each call |
| `/usr/share/polkit-1/actions/org.privateai.manager.policy` | `org.privateai.manager.control` (start, stop, restart): `yes` in the active session, `auth_admin` otherwise; `org.privateai.manager.configure` (writing a file): `auth_admin_keep` |
| `/usr/lib/sysusers.d/private-ai.conf` | The system user `private-ai`, home `/var/lib/private-ai`, in `render` and `video` |
| `/usr/lib/systemd/system/private-ai-bff.service` (`bff`) | `pai-bff`, `Type=exec`, as the static user and group `private-ai-bff`, hardened (`ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `RemoveIPC=yes`, `NoNewPrivileges=yes`, `RestrictSUIDSGID=yes`), with no capability, no device, the kernel and cgroups out of reach and only `AF_UNIX`, `AF_INET`, `AF_INET6` sockets; it reads `/etc/private-ai/bff.ini` and writes nothing; `Restart=on-failure`, enabled for `multi-user.target` |
| `/usr/lib/sysusers.d/private-ai-bff.conf` (`bff`) | The system user `private-ai-bff`, with no home and no shell |

**The bff runs as a static user, not a `DynamicUser=`:** it calls the manager on the system bus, and
dbus-daemon drops a dynamic user's connection at `Hello`. On Ubuntu 26.04 (dbus-daemon 1.16.2) a
`DynamicUser=yes` bff answered `/api/instances` with 503; the reproduction and the upstream reports
are in the unit's test in `system_files.rs`. The sandbox `DynamicUser=` implied is written out instead.

Their texts (the unit descriptions, the polkit descriptions and messages) are read in place by systemd
and polkit, so they are literal in the code, not in the strings files.

**The maintainer scripts are `core`'s, `llama.cpp`'s and `bff`'s**, in debhelper's form for the `.deb`
(`postinst`, `prerm`, `postrm`) and Fedora's systemd macros written out for the `.rpm` (`%pre`, `%post`,
`%preun`, `%postun`, `%posttrans`). **Each package stops the services it holds before its files are
replaced, on upgrade as on removal, and an upgrade starts again what was running.** The `.deb` stops in
`prerm` (`upgrade`, `remove`, `deconfigure`; not under `DPKG_ROOT`), the `.rpm` on an upgrade in the new
package's `%pre`, since rpm runs the old package's `%preun` only after the new files are in place
(rpm-scriptlets(7), EXECUTION ORDER).

| When | `core`: the manager | `llama.cpp`: the instances |
| --- | --- | --- |
| Install | `systemd-sysusers private-ai.conf`, enable the manager, `daemon-reload`, reload the bus policies, start the manager (the `.rpm` in `%posttrans`, once the old package's files are gone) | Copy the missing defaults into `/etc/llama.cpp/` (the `.rpm` in `%posttrans`); `daemon-reload`; no instance is enabled or started |
| Upgrade | Stop the manager; afterwards the same as an install, the manager restarted (the `.rpm` only while it is enabled) | Record the active `llama.cpp@*` units in `/run/llama.cpp.instances` and stop them; afterwards copy the missing defaults, `daemon-reload`, start each recorded unit, delete the record |
| Removal | Stop the manager; `daemon-reload` (the `.rpm` also disables it) | Stop every instance; `daemon-reload`, delete the record; `/etc/llama.cpp/` is kept. The `.rpm` also disables each instance linked in `/etc/systemd/system/multi-user.target.wants/` (on the last erase only, not on an upgrade); the `.deb` keeps the enablement, as Debian does for a package's own units |
| Purge (`.deb`) | Forget the manager's enablement state; `/etc/private-ai` and `/var/lib/private-ai` are kept | Remove `/etc/llama.cpp/` and the instances' `multi-user.target.wants/llama.cpp@*.service` links, which deb-systemd-helper does not know since the manager made them |

`core`'s scripts never touch the instances, and none enables one: an instance starts at boot only once
the user enables it. `bff`'s scripts do the same as `core`'s for `private-ai-bff.service` alone:
`systemd-sysusers private-ai-bff.conf`, then enable and start it on install, stop it before an upgrade
and restart it after (the `.rpm` in `%posttrans`, while it is enabled), stop it on removal (the `.rpm`
also disables it), and forget its enablement state on purge. Under `DPKG_ROOT` (a chroot install)
`llama.cpp`'s copy of the defaults and its purge stay inside the root, and the copy asks the host for
no group, so the server file is copied 0644 there. These scripts run in xtask's tests in a sandbox with a fake `systemctl`, the
chroot install included.

The `.deb` `Depends` on `systemd | systemd-standalone-sysusers | systemd-sysusers` for those scripts;
the `.rpm` of `core` and of `bff` `Requires(pre)`, `(post)`, `(preun)`, `(postun)` and `(posttrans)` systemd (`llama.cpp`'s the same but `(post)`, a scriptlet it does not have), and `core`'s takes weak dependencies on the `render` and `video` groups, which the sysusers `m` lines
create when missing.

<details>
<summary>Dependencies, payload and metadata</summary>

**`.deb` `Depends`** come from `dpkg-shlibdeps` over each package's staged ELF files, with the library
directories of all eight given as private paths, so a library another package carries asks for nothing.
`libvulkan1` is dropped because the loader is bundled.

- `libggml-cuda.so` and the `libcu*` files are left out of the scan. They link the driver's
  `libcuda.so.1`, which no Debian package provides, and `dpkg-shlibdeps` fails on a library it
  cannot find.
- A fixed list is the fallback of every package but the runtimes (font libraries for `config` alone). A runtime asks for nothing it does not bring along.
- What the configurator only `dlopen`s (xkbcommon, Wayland, X11, EGL) is `Recommends` of `config` alone:
  neither the Vulkan loader nor the Vulkan backend names those libraries.

**`.rpm`** keeps rpm's automatic scan.

- `__requires_exclude` covers every library of the eight, and `libcuda`.
- Those libraries are excluded from `Provides` too.
- `rpmbuild` runs against a throwaway `_dbpath`, since a Debian host has no rpm database.

**Payload and modes.** The payload is zstd in both formats, because xz takes many minutes on 3 GiB
of GPU kernels. Modes are normalized to 755 and 644, because the repo drive may hand out
group-writable ones.

**Icon.** It is the largest PNG frame of `resources/llama.ico`, cut out of the container by xtask,
so no image tool is needed.

**Licence metadata.** The `.rpm` `License:` field is computed from what each package bundles: an SPDX
expression, with `LicenseRef-NVIDIA-CUDA-EULA` for NVIDIA's terms. `usr/share/doc/<pkg>/copyright`
includes that package's `share/licenses/<package>/README.txt` and points to the directory.

**Maintainer.** `PRIVATE_AI_MAINTAINER` sets it, else git's `user.name` and `user.email`.

</details>

## Install

**`xtask install` covers first install, upgrade, same-version reinstall and downgrade with one
command.** On Windows it takes the last-written MSI of llama.cpp in `dist/`, then the last-written
of the framework (only those there), or the files given, at most one of each, llama.cpp's first. On
Linux and macOS it takes the last-written file of each package, or only the files given: that is how to
leave out the runtime of a GPU the machine does not have. `--dry-run` prints the commands.

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Command | `msiexec /i <msi> /qb /norestart /l*v <log>`, per MSI, llama.cpp's first | Package manager, see below | `sudo installer -pkg <pkg> -target /`, per package, the engine's first |
| Downgrade | Removes the installed version first: `msiexec /x {ProductCode} /qb`, llama.cpp's with `KEEPINSTANCES=1` | Package manager's own | Laid down over it |
| Running programs | The framework's `StopProcesses` action stops `pai` and `pai-config`; llama.cpp's MSI stops and restarts its running instances, and leaves a hand-started llama-server to the files-in-use dialog | A llama-server outside the services is refused (`--force` overrides) | A program from `/opt/llama.cpp/bin` or `/opt/private-ai/bin` is refused (`--force` overrides) |
| Success check | `DisplayVersion` matches the package | `pai-config --version`, when `config` is among the files | `pai-config --version`, when `config` is among the files |

**Windows.**

- Basic UI, not `/qn`: the package is per-machine, and only a UI level that can show the UAC prompt
  elevates from an unelevated shell.
- All features go to the `InstallDir` an earlier install recorded, or the default. For another
  choice, run the `.msi` itself.
- An upgrade and a same-version repackage are each MSI's own `MajorUpgrade`.
- Each MSI refuses a downgrade, hence the removal of that product first. The ProductCode comes from
  its Apps entry's `UninstallString`, and basic UI keeps the user data and `%ProgramData%\llama.cpp\`.
  The engine's removal passes `KEEPINSTANCES=1`: the instance services stay, the running ones are
  recorded and stopped, and the older release starts them again once installed.
- Success reads `DisplayVersion` of the product's Apps entry, `HKLM\...\Uninstall\llama.cpp` or
  `Uninstall\private-ai`. Exit 3010 (restart required) counts as success with a warning.
- The logs are `build\windows\msi-install-<product>.log` and `msi-uninstall-<product>.log`
  (`llama.cpp`, `private-ai-framework`).
- None of this has run on Windows since the split: type-checked only.

**Linux.**

| Format | Command |
| --- | --- |
| `.deb` | `apt-get install -y --reinstall --allow-downgrades`, every file in one transaction |
| `.rpm` with dnf | `dnf install`, `downgrade` and `reinstall`, one command per action, the rising versions first |
| `.rpm` with zypper | `zypper --non-interactive install --force --allow-unsigned-rpm`, every file in one transaction |
| `.rpm` otherwise | `rpm -Uvh --replacepkgs --oldpackage`, every file in one transaction |

- The format comes from `ID` and `ID_LIKE` in `/etc/os-release`, and the package managers on PATH
  only second, since a Debian build host has `rpm` too.
- A running llama-server is refused because rocBLAS and hipBLASLt load their kernel files lazily: a
  server outliving the upgrade would read the new kernels with the old libraries. Only a
  `llama-server` outside the framework's services counts, one run from `/usr/bin` or mapping a file
  of `/usr/lib/private-ai` (`/proc/<pid>/cmdline`, `/proc/<pid>/maps`). A process in the cgroup of a
  system unit `llama.cpp@*.service` or `private-ai-*.service` under `/system.slice/`
  (`/proc/<pid>/cgroup`) is a service, which its package's scripts stop and start again. Any other
  program (an open `pai-config`, a `pai bench run`, a `llama-bench`) is ignored without a message.
- It warns when a package is older than what its own product built: each framework package than its
  executable in `cargo-target/release/` (`config` `pai-config`, `cli` `pai`, `bff` `pai-bff`, `core`
  `pai-manager`), the engine than `llama.cpp-cmake/bin/llama-server` (`is_stale` in
  `xtask/src/linux/install.rs`). A runtime gets none: it comes out of an SDK, which the build does not
  change.
- `sudo` is added when not root, here and on macOS.
