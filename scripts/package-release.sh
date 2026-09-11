#!/bin/sh
set -eu
export COPYFILE_DISABLE=1

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
OUT=${OUT:-$ROOT/artifacts/release/balatro-miyoo-mini}
ARCHIVE=${ARCHIVE:-$ROOT/artifacts/release/balatro-miyoo-mini.zip}
RUNTIME=${BALATRO_RUNTIME:-$ROOT/target/armv7-unknown-linux-musleabihf/release/balatro-runtime}
AUDIO_HELPER=${BALATRO_AUDIO_HELPER:-$(dirname -- "$RUNTIME")/balatro-audio}
# shellcheck source=scripts/lib/package.sh
. "$ROOT/scripts/lib/package.sh"

if [ "${SKIP_BUILD:-0}" != 1 ]; then
    "$ROOT/scripts/build.sh"
fi
[ -f "$RUNTIME" ] || { echo "runtime was not built" >&2; exit 1; }
[ -f "$AUDIO_HELPER" ] || { echo "audio helper was not built" >&2; exit 1; }
command -v zip >/dev/null 2>&1 || { echo "zip is required" >&2; exit 1; }

prepare_output
copy_runtime
cp "$ROOT/port/README.txt" "$STAGING/README.txt"
"$ROOT/scripts/verify-package.sh" "$STAGING" runtime-only

mkdir -p "$(dirname -- "$ARCHIVE")"
ARCHIVE=$(CDPATH='' cd -- "$(dirname -- "$ARCHIVE")" && pwd)/$(basename -- "$ARCHIVE")
case "$ARCHIVE" in
    "$OUT"|"$OUT"/*) echo 'Archive must be outside the package directory' >&2; exit 1 ;;
esac
ARCHIVE_TEMP=$(mktemp "$ARCHIVE.tmp.XXXXXX")
(cd "$STAGING" && zip -qr - . -x '*/._*' '.DS_Store') >"$ARCHIVE_TEMP"
unzip -tqq "$ARCHIVE_TEMP"
install_output
mv "$ARCHIVE_TEMP" "$ARCHIVE"
ARCHIVE_TEMP=
printf '%s\n' "$ARCHIVE"
