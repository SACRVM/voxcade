#!/usr/bin/env bash
# Fails if `cargo test --release` leaves anything named voxcade-* in the
# temp folder (here it is RAM: two days of leftovers once filled it).
# Tests take their folders from `scratch::Scratch`, which removes them.
#
# usage: tools/check_tmp.sh [cargo test arguments]
set -u
cd "$(git rev-parse --show-toplevel)" || exit 2
tmp="${TMPDIR:-/tmp}"
count() { find "$tmp" -maxdepth 1 -name 'voxcade-*' | wc -l; }
before=$(count)
cargo test --release "$@" || { echo "check_tmp: the tests fail"; exit 1; }
after=$(count)
if [ "$after" -gt "$before" ]; then
  echo "check_tmp: $((after - before)) new voxcade-* entries in $tmp (use scratch::Scratch):"
  find "$tmp" -maxdepth 1 -name 'voxcade-*' -newer Cargo.toml | sed 's/-[0-9]*-[0-9]*\(\.[a-z]*\)\{0,1\}$//' | sort | uniq -c
  exit 1
fi
echo "check_tmp: no leftovers in $tmp ($before before, $after after)"
