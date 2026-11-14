#requires -Version 7
# Build llama.cpp (CMake + Ninja, CUDA + Vulkan + HIP) and the framework
# (the web UI, then pai-config, pai, pai-manager and pai-bff with cargo). Two separate modules: xtask\src\windows\build_llama.rs and
# xtask\src\common\configurator.rs; this script only runs them.
# docs\build-compiling.md explains the decisions behind the build.
#
# Usage: .\scripts\02-build.ps1 [llama | framework]   (default: both, in that order)
# The same as: cargo run -p xtask -- build [llama | framework]
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$env:CARGO_TARGET_DIR = Join-Path $repo 'build\windows\cargo-target'
cargo run --quiet --manifest-path (Join-Path $repo 'Cargo.toml') -p xtask -- build @args
exit $LASTEXITCODE
