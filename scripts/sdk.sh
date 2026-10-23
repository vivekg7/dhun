#!/usr/bin/env bash
#
# Runs a command in linux/ inside the GNOME SDK with Rust (plan 039), for a
# machine with no Rust or GTK development packages of its own. The Makefile
# uses it when `cargo` is not on the PATH.
#
#   ./scripts/sdk.sh cargo test
#
# Needs the SDK once: flatpak install --user flathub org.gnome.Sdk//51 \
#   org.freedesktop.Sdk.Extension.rust-stable//26.08

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
exec flatpak run --user --share=network --filesystem="$ROOT" \
    --env=FLATPAK_ENABLE_SDK_EXT=rust-stable --env=CARGO_HOME="$ROOT/linux/target/cargo-home" \
    --command=bash org.gnome.Sdk//51 -c '. /usr/lib/sdk/rust-stable/enable.sh; cd "$0" && "$@"' "$ROOT/linux" "$@"
