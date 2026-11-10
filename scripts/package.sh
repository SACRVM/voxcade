#!/bin/bash
# Puts this system's package together in builds/voxcade-linux, as the
# Builds workflow ships it (tools/package.py), without SDL built in.
# For the Firepit command "Build package".
set -e
cd "$(dirname "$0")/.."
cargo build --release 2>&1 | tail -1
rm -rf builds/voxcade-linux
python3 tools/package.py --system linux --target x86_64-unknown-linux-gnu --features "" \
  --program target/release/voxcade --out builds/voxcade-linux
