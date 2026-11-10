#!/usr/bin/env bash
# The standard pass after a change to Voxcade: the release build, the
# tests, clippy, and every game's replays. Prints only what needs looking
# at, and a last line saying whether all passed.
#
# usage: verify.sh [--no-clippy] [GAME ...]     (games: names under games/;
#                                                 default: every game with replays)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 2

clippy=1
games=()
for a in "$@"; do
  case "$a" in
    --no-clippy) clippy=0 ;;
    *) games+=("$a") ;;
  esac
done
if [ ${#games[@]} -eq 0 ]; then
  for d in games/*/replays; do games+=("$(basename "$(dirname "$d")")"); done
fi
bad=0

echo "== build (release)"
if ! out=$(cargo build --release 2>&1); then
  echo "$out" | grep -E "^error" -A8 | head -60
  echo "RESULT: the build fails"; exit 1
fi
echo "$out" | grep -E "^warning" -A6 | head -30

echo "== tests"
out=$(cargo test --release 2>&1)
echo "$out" | grep -E "^test result" | head -1
if echo "$out" | grep -qE "FAILED|panicked|^error"; then
  echo "$out" | grep -E "^---- .* stdout ----|panicked at|^error" -A3 | head -60
  bad=1
fi

if [ $clippy -eq 1 ]; then
  echo "== clippy (every warning, file:line; compare with what was there before)"
  cargo clippy --release --all-targets 2>&1 | grep -E "^(warning|error)" -A4 \
    | grep -E "^(warning|error)|-->" | grep -v "generated [0-9]* warning" \
    | paste - - | sed -E 's/ +--> / @ /' | sort -u
fi

echo "== replays"
for g in "${games[@]}"; do
  out=$(./target/release/voxcade check "games/$g" 2>&1)
  total=$(echo "$out" | grep -c "\.toml:")
  same=$(echo "$out" | grep -c ": same")
  echo "$g: $same of $total same"
  if [ "$same" -ne "$total" ] || [ "$total" -eq 0 ]; then
    echo "$out" | grep -v ": same" | sed 's/^/  /'
    bad=1
  fi
done

if [ $bad -eq 0 ]; then echo "RESULT: all passed"; else echo "RESULT: something to look at (above)"; exit 1; fi
