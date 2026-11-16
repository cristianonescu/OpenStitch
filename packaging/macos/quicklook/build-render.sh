#!/bin/sh
# Builds the Rust renderer the Quick Look extension links (render/, #1182) as one universal
# static library (arm64 + x86_64):
#   packaging/macos/quicklook/build-render.sh <out/libopenstitch_quicklook.a>
# Needs the aarch64-apple-darwin and x86_64-apple-darwin Rust targets.
set -eu

out=$1
here=$(cd "$(dirname "$0")" && pwd)
manifest="$here/render/Cargo.toml"
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --lib --target "$t" --manifest-path "$manifest"
done
mkdir -p "$(dirname "$out")"
lipo -create -output "$out" \
    "$here/render/target/aarch64-apple-darwin/release/libopenstitch_quicklook.a" \
    "$here/render/target/x86_64-apple-darwin/release/libopenstitch_quicklook.a"
lipo -info "$out"
