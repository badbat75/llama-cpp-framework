#requires -Version 7
# Install the newest MSI the package phase left in .\dist\ on this machine
# (basic UI with the UAC prompt; install, upgrade, same-version repackage or
# downgrade alike). The logic is in xtask\src\windows\install.rs; this script
# only runs it.
#
# Usage: .\scripts\04-install.ps1 [--dry-run] [<package file>]
# The same as: cargo run -p xtask -- install [--dry-run] [<package file>]
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$env:CARGO_TARGET_DIR = Join-Path $repo 'build\windows\cargo-target'
cargo run --quiet --manifest-path (Join-Path $repo 'Cargo.toml') -p xtask -- install @args
exit $LASTEXITCODE
