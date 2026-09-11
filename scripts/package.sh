#!/bin/sh
set -eu
export COPYFILE_DISABLE=1

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
GAME=${1:-${BALATRO_GAME:-}}
OUT=${OUT:-$ROOT/artifacts/balatro-miyoo}
RUNTIME=${BALATRO_RUNTIME:-$ROOT/target/armv7-unknown-linux-musleabihf/release/balatro-runtime}
AUDIO_HELPER=${BALATRO_AUDIO_HELPER:-$(dirname -- "$RUNTIME")/balatro-audio}
# shellcheck source=scripts/lib/package.sh
. "$ROOT/scripts/lib/package.sh"

[ -n "$GAME" ] || { echo 'Usage: scripts/package.sh /path/to/Balatro.exe' >&2; exit 1; }
[ -f "$GAME" ] || { echo "Balatro game file not found: $GAME" >&2; exit 1; }

if [ "${SKIP_BUILD:-0}" != 1 ]; then
    "$ROOT/scripts/build.sh"
fi
[ -f "$RUNTIME" ] || { echo "runtime was not built" >&2; exit 1; }
[ -f "$AUDIO_HELPER" ] || { echo "audio helper was not built" >&2; exit 1; }
if [ "${PREPARE_AUDIO:-1}" = 1 ]; then
    PCM_CACHE_DIR=${PCM_CACHE_DIR:-$ROOT/artifacts/audio-cache}
    cargo run --locked --quiet --release --manifest-path "$ROOT/Cargo.toml" \
        -p balatro-runtime -- --prepare-audio "$GAME" "$PCM_CACHE_DIR"
fi

prepare_output
copy_runtime
cp "$GAME" "$GAME_DIR/Balatro"
if [ -n "${PCM_CACHE_DIR:-}" ]; then
    [ -d "$PCM_CACHE_DIR" ] || { echo "Audio cache not found: $PCM_CACHE_DIR" >&2; exit 1; }
    cp -R "$PCM_CACHE_DIR" "$GAME_DIR/audio-cache"
fi
"$ROOT/scripts/verify-package.sh" "$STAGING"
install_output
printf '%s\n' "$OUT"
