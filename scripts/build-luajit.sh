#!/bin/sh
set -eu

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
VERSION=210.5.12+a4f56a4
OUT=${BALATRO_LUAJIT_DIR:-$ROOT/target/luajit-armv7-musl}
TARGET_CFLAGS=${BALATRO_LUAJIT_TARGET_CFLAGS:-}
CONFIG=$OUT/build-config

command -v cargo >/dev/null 2>&1 || { echo "cargo is required" >&2; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "docker is required" >&2; exit 1; }

if [ -f "$OUT/libluajit-5.1.a" ] && [ -f "$OUT/libgcc.a" ]; then
    if [ -z "$TARGET_CFLAGS" ] && [ ! -f "$CONFIG" ]; then
        exit 0
    fi
    if [ -f "$CONFIG" ] &&
        [ "$(sed -n '1p' "$CONFIG")" = "version=$VERSION" ] &&
        [ "$(sed -n '2p' "$CONFIG")" = "target_cflags=$TARGET_CFLAGS" ]; then
        exit 0
    fi
    echo "LuaJIT output already exists with different build settings: $OUT" >&2
    exit 1
fi

cd "$ROOT"
SOURCE=$ROOT/vendor/luajit-src-$VERSION
if [ ! -d "$SOURCE" ]; then
    cargo fetch --locked >/dev/null
    SOURCE=$(find "$CARGO_HOME/registry/src" -maxdepth 2 -type d -name "luajit-src-$VERSION" -print -quit)
fi
[ -n "$SOURCE" ] || { echo "LuaJIT source was not found in the Cargo cache" >&2; exit 1; }

mkdir -p "$OUT"
docker run --rm --platform linux/arm/v7 \
    -e "BALATRO_LUAJIT_TARGET_CFLAGS=$TARGET_CFLAGS" \
    -v "$SOURCE/luajit2:/input:ro" \
    -v "$OUT:/out" \
    arm32v7/debian:buster sh -lc '
set -eu
export DEBIAN_FRONTEND=noninteractive
printf "%s\n" \
    "deb http://archive.debian.org/debian buster main" \
    "deb http://archive.debian.org/debian-security buster/updates main" \
    > /etc/apt/sources.list
printf "%s\n" "Acquire::Check-Valid-Until false;" > /etc/apt/apt.conf.d/99archive
apt-get update >/dev/null
apt-get install -y build-essential musl-tools >/dev/null
cp -a /input /build
make -C /build -j2 BUILDMODE=static CC=musl-gcc HOST_CC=musl-gcc \
    STATIC_CC=musl-gcc TARGET_LD=musl-gcc \
    TARGET_CFLAGS="$BALATRO_LUAJIT_TARGET_CFLAGS" >/dev/null
cp /build/src/libluajit.a /out/libluajit-5.1.a
cp "$(musl-gcc -print-libgcc-file-name)" /out/libgcc.a
'

{
    printf 'version=%s\n' "$VERSION"
    printf 'target_cflags=%s\n' "$TARGET_CFLAGS"
} >"$CONFIG"

printf '%s\n' "$OUT"
