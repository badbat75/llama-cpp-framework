#requires -Version 7
# Detect paths and tools and write build\windows\config-build.psd1 and the
# generated .cargo\config.toml. Run it after scripts\00-install-prerequisites.ps1 and
# again whenever a dist or the toolchain changes. The logic is xtask's
# configure phase (xtask\src\windows\configure.rs); this script only runs it.
#
# Usage: .\scripts\01-configure.ps1 [--llama-cpp-dir <path>] [--dist-root <path>]
# The same as: cargo run -p xtask -- configure [--llama-cpp-dir <path>] [--dist-root <path>]
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$env:CARGO_TARGET_DIR = Join-Path $repo 'build\windows\cargo-target'
cargo run --quiet --manifest-path (Join-Path $repo 'Cargo.toml') -p xtask -- configure @args
exit $LASTEXITCODE
