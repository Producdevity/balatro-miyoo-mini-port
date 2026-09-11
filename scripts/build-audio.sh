#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT=$ROOT/target/armv7-unknown-linux-musleabihf/release
mkdir -p "$OUT"
zig cc -target arm-linux-gnueabihf.2.23 -mcpu=cortex_a7 -O2 -s \
    -Wall -Wextra -Werror "$ROOT/native/onion-audio.c" -o "$OUT/balatro-audio"
