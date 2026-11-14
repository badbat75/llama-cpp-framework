---
title: Building on macOS
type: reference
updated: 2026-11-14
---

# Building on macOS

The macOS build from prerequisites to package, kept in one place because it differs from the other
systems in every phase. What it shares with them is linked where it applies, and the phases are in
[Build pipeline](build-pipeline.md).

**The macOS build is Apple Silicon only and untested.** It was written on Linux and compile-checked
for `aarch64-apple-darwin` only, so the first run on a Mac is its test. `configure` refuses another
architecture. The phases are in `xtask/src/macos/`, and everything generated goes to `build/macos/`.

**Backends.** Metal is part of the system, and Vulkan runs through MoltenVK. No CUDA or ROCm exists
there. The CPU backend keeps `GGML_CPU_ALL_VARIANTS`, which upstream implements on Apple as
`apple_m1`, `apple_m2_m3` and `apple_m4`.

`xtask/cmake/macos.toml` follows upstream's own macOS release build:

| Option | Why |
| --- | --- |
| `GGML_CUDA=OFF`, `GGML_HIP=OFF` | Neither exists there |
| `GGML_METAL=ON` | Stated rather than inherited |
| `GGML_METAL_EMBED_LIBRARY=ON` | No `default.metallib` to find at run time |
| `CMAKE_OSX_ARCHITECTURES=arm64` | Apple Silicon only |
| `CMAKE_OSX_DEPLOYMENT_TARGET=13.3` | The oldest macOS the binaries run on, upstream's value |
| `LLAMA_BUILD_BORINGSSL=ON` | HTTPS through a static BoringSSL fetched at configure time: no OpenSSL dylib, no Homebrew path |

**Prerequisites.** `scripts/00-install-prerequisites.sh` does these in order:

1. Checks the Xcode Command Line Tools with `xcode-select -p`. Their installer is a GUI dialog, so
   it is reported and not run.
2. Installs the Homebrew packages `cmake ninja node` (`--skip brew`). Homebrew itself is checked,
   not installed.
3. Installs Rust as [on Linux](build-prerequisites.md#linux-prerequisites).
4. Runs `xtask prereq`: sccache (the `aarch64-apple-darwin` asset) and the Vulkan SDK.

**Dist layout.** The layout is `<root>/VulkanSDK/<Version>/macos`, with the root defaulting to
`~/dists`, [configurable as on the other systems](build-prerequisites.md#dist-layout-and-root).

- The newest version is LunarG's `sdk/latest/mac.txt`.
- The zip `vulkansdk-macos-<version>.zip` is checked against `files.json` (`mac` key) and unpacked
  with `ditto`. It needs 4 GiB free.
- Its installer app runs unattended:
  `--root <dir> --accept-licenses --default-answer --confirm-command install copy_only=1`.
  `copy_only=1` writes nothing to `/usr/local` and needs no sudo.
- `find_vulkan` (`xtask/src/macos/dists.rs`) requires the marker, `vulkan.h`, `glslc`,
  `libvulkan.1.dylib` and the MoltenVK manifest.

**Configure** writes `build/macos/config-build.toml`: Apple's `clang` and `clang++` by full path
(`xcrun --find`), the dist root, the clone, build type and jobs, and `minimal` for
`configure --minimal` ([the minimal build](build-compiling.md#the-minimal-build): Metal only, no
Vulkan, for CI).

**Build** uses the [shared checkout and patches](build-compiling.md#release-tag-and-patches) and
[options](build-compiling.md#cmake-options), Apple's clang and Ninja. The Vulkan SDK's include
directory, loader and `glslc` are
[pinned as on Linux](build-compiling.md#pinning-the-vulkan-sdk-and-the-cuda-toolkit). The build
tree's binaries find the SDK's loader through their build RPATH. A changed compiler path wipes the build directory, since a
new Xcode can move it.

**Package.** Type-checked only: `xtask package` and `xtask install` on macOS have never run on a Mac,
and no `pkgbuild` has run here; what follows is read from the code (`xtask/src/macos/package.rs`,
`install.rs` and the pure `xtask/src/common/macos_pkg.rs`, whose tests run on Linux). As
[on Linux](build-packaging.md#packaging), the engine is packaged apart from the framework, each in its
own prefix: llama.cpp in `/opt/llama.cpp` with its Vulkan runtime, the framework in `/opt/private-ai`.
`pkgbuild` writes five `.pkg` files into `dist/`, split by path by `components.rs`:

| Package | File | Identifier | Version |
| --- | --- | --- | --- |
| The engine | `llama.cpp-v<release>-arm64.pkg` | `org.privateai.llama.cpp` | The llama.cpp release |
| Its Vulkan runtime | `llama.cpp-vulkan-v<sdk>-arm64.pkg` | `org.privateai.llama.cpp.vulkan` | The SDK's, or the framework's for an SDK outside the layout |
| `config` (the GUI) | `private-ai-framework-config-v<framework>-arm64.pkg` | `org.privateai.framework.config` | The framework's |
| `cli` (`pai`) | `private-ai-framework-cli-v<framework>-arm64.pkg` | `org.privateai.framework.cli` | The framework's |
| `core` | `private-ai-framework-core-v<framework>-arm64.pkg` | `org.privateai.framework.core` | The framework's |

For example `llama.cpp-v1.0.0-arm64.pkg` and `private-ai-framework-cli-v2.99.1-arm64.pkg` (the
`macos_pkg` tests). The engine's file has no `+pai<N>`: no system package shares its name. macOS has no
manager and no web service, so no `bff` package, and `core` holds only `cli.toml`. A minimal build
packages four: no `llama.cpp-vulkan`, and the engine is `llama.cpp-v<release>-minimal-arm64.pkg`.

`xtask package llama` or `framework` builds one product's packages and leaves the other's files in
`dist/`, as `xtask build llama` and `build framework` build one. The engine needs `llama-server` in the
build's `bin/`; the framework needs `pai-config` and `pai` there (`xtask build framework` copies them)
and refuses a built `pai-config` whose version differs from the root `Cargo.toml`'s. For either product
it refuses, as on Linux and Windows, a llama.cpp clone not on a `vX.Y.Z` release tag.

| Path | Package | Holds |
| --- | --- | --- |
| `/opt/llama.cpp/bin/` | The engine | llama.cpp's executables, libraries and backends, without `test-*` |
| `/opt/llama.cpp/lib/` | Vulkan | The Vulkan loader and MoltenVK (`libMoltenVK.dylib`) |
| `/opt/llama.cpp/share/licenses/llama/`, `vulkan/` | Each its own | The licence texts, with the MoltenVK and Vulkan loader ones in `vulkan/`; see [Packaging](build-packaging.md#packaging) |
| `/usr/local/share/vulkan/icd.d/llama.cpp-MoltenVK_icd.json` | Vulkan | The driver manifest, with an absolute `library_path` to `/opt/llama.cpp/lib/libMoltenVK.dylib` |
| `/opt/private-ai/bin/` | `pai-config` in `config`, `pai` in `cli` | The framework's two executables |
| `/opt/private-ai/share/benchmarks/` | `cli` | The benchmark method |
| `/opt/private-ai/share/strings/` | `config`; `cli.toml` in `core` | The GUI's `en_US.toml`, and `cli.toml`, the text `pai` prints (so `pai` needs `core`) |
| `/opt/private-ai/share/licenses/<suffix>/` | Each its own | The licence texts |
| `/usr/local/bin/` | Each its own executables' | A symlink per executable, into its own prefix |

- `pai-config` and `pai` look for llama-server beside themselves and in the build trees (under
  `build/macos/`, `paths::build_os_dir`) first, then in `/opt/llama.cpp/bin/` when that holds
  `llama-server` (`paths::engine_install_dir`, pinned on Linux over a temporary root).
- The manifest goes to `icd.d` because the loader does not look next to itself. A system-wide Vulkan
  SDK install adds its own MoltenVK manifest there too.
- The bundled libraries are the `otool -L` closure of the staged files restricted to the SDK, copied
  into the engine's `lib/`. Any other non-system dependency, such as a Homebrew dylib, fails the
  phase, and so does an SDK dependency of a framework file.
- Every staged Mach-O loses its build rpaths and gets `@loader_path`, plus `@loader_path/../lib` in
  the engine's `bin/`.
- Each then gets an ad hoc signature (`codesign --sign -`), because arm64 runs no unsigned or
  modified-and-unsigned code.
- The packages are unsigned and not notarized, so Gatekeeper blocks them when downloaded through a
  browser.

**A `.pkg` has no dependency mechanism, so nothing ties the five together.** `xtask install` takes the
newest of each in `dist/` with `installer`, or only the files given; it takes no product. It lays the
engine's down first, then the framework's. It refuses while a program runs from `/opt/llama.cpp/bin/`
or `/opt/private-ai/bin/` (`--force` goes ahead), and checks `pai-config --version` when `config` is
among the files. Without the Vulkan package ggml skips that backend and Metal runs.

**Open, by the code** (`PLAN.md` step 11):

- A release 2.4 install is not removed. 2.4 installed everything into `/opt/llama.cpp` (its
  `llama-cpp-config` included) under 2.4's own identifiers, with the
  manifest `llama-cpp-framework-MoltenVK_icd.json`; the engine's `.pkg` lays down over that tree and
  leaves 2.4's own files, receipts and manifest in place.

**The configurator** builds with its existing non-Linux, non-Windows fallbacks: start at login, GPU
monitoring and similar are not ported. Its paths follow the Linux ones (`~/.local/share/private-ai`,
`~/.config/private-ai`) per `paths.rs`, and so does the move of release 2.4's trees at its start.
