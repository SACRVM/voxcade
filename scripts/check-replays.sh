#!/bin/bash
# Plays every game's replays without a window (voxcade check): each must
# run as recorded. For the Firepit command "Test replays".
set -e
cd "$(dirname "$0")/.."
cargo build --release 2>&1 | tail -1
for game in games/*/; do
  echo "== ${game%/}"
  ./target/release/voxcade check "${game%/}" | tail -3
done
