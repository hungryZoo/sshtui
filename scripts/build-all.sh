#!/usr/bin/env bash
# Build the two macOS and four Linux targets from a macOS development machine.
set -euo pipefail

cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=11.0

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --locked --release --target "$target"
done
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
              armv7-unknown-linux-musleabihf arm-unknown-linux-musleabihf; do
    cargo zigbuild --locked --release --target "$target"
done
