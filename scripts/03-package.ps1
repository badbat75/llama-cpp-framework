#requires -Version 7
# Package llama.cpp and the framework into an MSI each in .\dist\ (WiX
# Toolset), validated (ICE). Requires a successful build and the WiX Toolset
# pinned in installer\wix-toolset.psd1 (scripts\00-install-prerequisites.ps1
# installs it). The logic is in xtask\src\windows\package.rs; this script only runs it.
#
# Usage: .\scripts\03-package.ps1 [llama | framework] [--keep-staging] [--wix <path to wix.exe>]
#        (default: both products)
# The same as: cargo run -p xtask -- package [llama | framework] [--keep-staging] [--wix <path to wix.exe>]
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$env:CARGO_TARGET_DIR = Join-Path $repo 'build\windows\cargo-target'
cargo run --quiet --manifest-path (Join-Path $repo 'Cargo.toml') -p xtask -- package @args
exit $LASTEXITCODE
