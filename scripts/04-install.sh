#!/usr/bin/env bash
# Install the newest package the package phase left in ./dist/ on this machine
# (install, upgrade, same-version reinstall or downgrade alike). The logic is
# in xtask/src/<os>/install.rs; this script only runs it.
#
# Usage: scripts/04-install.sh [--deb | --rpm] [--force] [--dry-run] [<package file>]
#        (--deb/--rpm: Linux; on macOS the package is the .pkg)
# The same as: cargo run -p xtask -- install [options]
set -euo pipefail
repo="$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")"
# build/linux or build/macos: everything generated stays per OS.
case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac
export CARGO_TARGET_DIR="$repo/build/$os/cargo-target"
PATH="$PATH:$HOME/.cargo/bin"
exec cargo run --quiet --manifest-path "$repo/Cargo.toml" -p xtask -- install "$@"
