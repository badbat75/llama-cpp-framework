#!/usr/bin/env bash
# Build llama.cpp (CMake + Ninja; CUDA + Vulkan + HIP on Linux, Metal +
# Vulkan/MoltenVK on macOS) and the framework (the web UI, then pai-config,
# pai, pai-manager and pai-bff with cargo), the counterpart of 02-build.ps1. Two separate modules: xtask/src/<os>/build_llama.rs and
# xtask/src/common/configurator.rs; this script only runs them.
# docs/build-compiling.md explains the decisions behind the build.
#
# Usage: scripts/02-build.sh [llama | framework]   (default: both, in that order)
# The same as: cargo run -p xtask -- build [llama | framework]
set -euo pipefail
repo="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
# build/linux or build/macos: everything generated stays per OS.
case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac
export CARGO_TARGET_DIR="$repo/build/$os/cargo-target"
PATH="$PATH:$HOME/.cargo/bin"
exec cargo run --quiet --manifest-path "$repo/Cargo.toml" -p xtask -- build "$@"
