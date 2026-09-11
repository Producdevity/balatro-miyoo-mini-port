#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT=${OUT:-$ROOT/artifacts/release}
mkdir -p "$OUT"
OUT=$(CDPATH='' cd -- "$OUT" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
SOURCE=$WORK/balatro-miyoo-mini
mkdir -p "$SOURCE/.cargo"

# Git excludes local game files, build output and developer notes.
git -C "$ROOT" archive HEAD | tar -x -C "$SOURCE"
(
    cd "$SOURCE"
    cargo vendor --locked --versioned-dirs vendor > .cargo/config.toml
)

SLEEF=$ROOT/target/sleef-armv7-musl
[ -d "$SLEEF/sleef-3.9.0" ] || {
    echo 'Run scripts/build-sleef.sh before packaging source' >&2
    exit 1
}
mkdir -p "$SOURCE/vendor/sleef"
cp -R "$SLEEF/sleef-3.9.0" "$SOURCE/vendor/sleef/"
cp "$SLEEF/sleef-3.9.0.tar.gz" "$SOURCE/vendor/sleef/"

tar -czf "$WORK/source.tar.gz" -C "$WORK" balatro-miyoo-mini
mv "$WORK/source.tar.gz" "$OUT/balatro-miyoo-mini-source.tar.gz"
printf '%s\n' "$OUT/balatro-miyoo-mini-source.tar.gz"
