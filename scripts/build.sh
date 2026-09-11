#!/bin/sh
set -eu

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
TARGET=armv7-unknown-linux-musleabihf
LUA_ROOT=${BALATRO_LUAJIT_DIR:-$ROOT/target/luajit-armv7-musl}

command -v cargo >/dev/null 2>&1 || { echo "cargo is required" >&2; exit 1; }
command -v cargo-zigbuild >/dev/null 2>&1 || { echo "cargo-zigbuild is required" >&2; exit 1; }

cd "$ROOT"
"$ROOT/scripts/build-luajit.sh"
sh "$ROOT/scripts/build-sleef.sh" >&2
export LUA_LIB="$LUA_ROOT"
export LUA_LIB_NAME=luajit-5.1
export LUA_LINK=static
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-cpu=cortex-a7 -C target-feature=+neon,+vfp4 -L native=$LUA_ROOT -l static=gcc"
cargo zigbuild --locked --release --workspace --target "$TARGET" --features arm-neon,flame-simd,layer-pairs "$@"
sh "$ROOT/scripts/build-audio.sh"
