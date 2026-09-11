#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT=${BALATRO_SLEEF_DIR:-$ROOT/target/sleef-armv7-musl}
VERSION=3.9.0
HASH=af60856abac08a3b5e72a8d156dd71fec1f7ac23de8ee67793f45f9edcdf0908
ARCHIVE=$OUT/sleef-$VERSION.tar.gz
SOURCE=$OUT/sleef-$VERSION
command -v zig >/dev/null || { echo 'zig is required' >&2; exit 1; }
mkdir -p "$OUT"
if [ ! -f "$ARCHIVE" ]; then
    if [ -f "$ROOT/vendor/sleef/sleef-$VERSION.tar.gz" ]; then
        cp "$ROOT/vendor/sleef/sleef-$VERSION.tar.gz" "$ARCHIVE.part"
    else
        curl -fL --retry 2 "https://github.com/shibatch/sleef/archive/refs/tags/$VERSION.tar.gz" -o "$ARCHIVE.part"
    fi
    mv "$ARCHIVE.part" "$ARCHIVE"
fi
actual=$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')
[ "$actual" = "$HASH" ] || { echo 'SLEEF archive checksum does not match' >&2; exit 1; }
tar -xzf "$ARCHIVE" -C "$OUT"
zig cc -target arm-linux-musleabihf -mcpu=cortex_a7 -mfpu=neon-vfpv4 \
    -O3 -ffp-contract=off -fno-math-errno -ffunction-sections -fdata-sections \
    -DENABLE_NEON32 -I"$ROOT/crates/renderer/src/sleef" \
    -I"$SOURCE/src/common" -I"$SOURCE/src/arch" \
    -c "$SOURCE/src/libm/sleefsimdsp.c" -o "$OUT/math.o"
zig cc -target arm-linux-musleabihf -mcpu=cortex_a7 \
    -O3 -ffunction-sections -fdata-sections -I"$ROOT/crates/renderer/src/sleef" \
    -I"$SOURCE/src/common" -c "$SOURCE/src/libm/rempitab.c" -o "$OUT/table.o"
zig ar rcs "$OUT/libbalatro_sleef.a" "$OUT/math.o" "$OUT/table.o"
printf '%s\n' "$OUT/libbalatro_sleef.a"
