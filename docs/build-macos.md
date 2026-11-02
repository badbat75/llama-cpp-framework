---
title: Building on macOS
type: reference
updated: 2026-11-01
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
(`xcrun --find`), the dist root, the clone, build type and jobs.

**Build** uses the [shared checkout and patches](build-compiling.md#release-tag-and-patches) and
[options](build-compiling.md#cmake-options), Apple's clang and Ninja. The Vulkan SDK's include
directory, loader and `glslc` are
[pinned as on Linux](build-compiling.md#pinning-the-vulkan-sdk-and-the-cuda-toolkit). The build
tree's binaries find the SDK's loader through their build RPATH. A changed compiler path wipes the build directory, since a
new Xcode can move it.

**Package.** `pkgbuild` writes the `.pkg` (identifier `io.github.badbat75.llama-cpp-framework`). It
refuses a built `llama-cpp-config` whose version differs from `Cargo.toml`.

| Path | Holds |
| --- | --- |
| `/opt/llama.cpp/bin/` | The build's `bin/` without `test-*`, `llama-cpp-config` beside the server |
| `/opt/llama.cpp/lib/` | The Vulkan loader and MoltenVK |
| `/opt/llama.cpp/share/benchmarks/` | The benchmark method |
| `/opt/llama.cpp/share/strings/` | The configurator's text: the GUI's `en_US.toml`, the CLI's `cli.toml` |
| `/opt/llama.cpp/share/licenses/` | The licence texts, with the MoltenVK and Vulkan loader ones; see [Packaging](build-packaging.md#packaging) |
| `/usr/local/bin/` | A symlink per executable |
| `/usr/local/share/vulkan/icd.d/llama-cpp-framework-MoltenVK_icd.json` | The driver manifest, with an absolute `library_path` into `/opt/llama.cpp/lib` |

- The manifest goes there because the loader does not look next to itself. A system-wide Vulkan SDK
  install adds its own MoltenVK manifest there too.
- The bundled libraries are the `otool -L` closure of the staged files restricted to the SDK. Any
  other non-system dependency, such as a Homebrew dylib, fails the phase.
- Every staged Mach-O loses its build rpaths and gets `@loader_path`, plus `@loader_path/../lib` in
  `bin/`.
- Each then gets an ad hoc signature (`codesign --sign -`), because arm64 runs no unsigned or
  modified-and-unsigned code.
- The package is unsigned and not notarized, so Gatekeeper blocks it when downloaded through a
  browser.

**The configurator** builds with its existing non-Linux, non-Windows fallbacks: start at login, GPU
monitoring and similar are not ported. Its paths follow the Linux ones (`~/.local/share/llama.cpp`,
`~/.config/llama.cpp`) per `paths.rs`.
