#!/bin/sh

prepare_output() {
    case "$OUT" in
        ''|/|.|..|*/.|*/..) echo "Invalid package directory: $OUT" >&2; exit 1 ;;
    esac
    mkdir -p "$(dirname -- "$OUT")"
    OUT=$(CDPATH='' cd -- "$(dirname -- "$OUT")" && pwd)/$(basename -- "$OUT")
    if [ -e "$OUT" ] && [ ! -f "$OUT/Roms/PORTS/Games/Balatro/balatro-runtime" ]; then
        echo "Refusing to replace a directory that is not a Balatro package: $OUT" >&2
        exit 1
    fi
    [ ! -L "$OUT" ] || { echo "Package directory must not be a symlink" >&2; exit 1; }
    STAGING=$(mktemp -d "$OUT.tmp.XXXXXX")
    PREVIOUS=$STAGING.previous
    ARCHIVE_TEMP=
    trap cleanup_package EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
}

cleanup_package() {
    [ -z "$STAGING" ] || rm -rf "$STAGING"
    [ -z "$ARCHIVE_TEMP" ] || rm -f "$ARCHIVE_TEMP"
    if [ -e "$PREVIOUS" ] && [ ! -e "$OUT" ]; then
        mv "$PREVIOUS" "$OUT"
    fi
}

copy_runtime() {
    GAME_DIR=$STAGING/Roms/PORTS/Games/Balatro
    mkdir -p "$GAME_DIR/licenses" "$STAGING/Roms/PORTS/Shortcuts/Strategy"
    cp "$RUNTIME" "$GAME_DIR/balatro-runtime"
    cp "$AUDIO_HELPER" "$GAME_DIR/balatro-audio"
    cp "$ROOT/port/script.sh" "$GAME_DIR/script.sh"
    cp "$ROOT/port/Balatro.notfound" "$STAGING/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
    cp "$ROOT/LICENSE" "$GAME_DIR/licenses/runtime-GPL-3.0.txt"
    for license in balatro-port-tui LuaJIT PortMaster SLEEF Rust musl GCC-exception-3.1 GPL-3.0-or-later Nunito; do
        cp "$ROOT/licenses/$license.txt" "$GAME_DIR/licenses/$license.txt"
    done
    cp "$ROOT/licenses/Rust-standard-library.html" "$GAME_DIR/licenses/Rust-standard-library.html"
    cp "$ROOT/NOTICE" "$GAME_DIR/NOTICE"
    chmod +x "$GAME_DIR/balatro-runtime" "$GAME_DIR/balatro-audio" \
        "$GAME_DIR/script.sh" "$STAGING/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
}

install_output() {
    [ ! -e "$OUT" ] || mv "$OUT" "$PREVIOUS"
    mv "$STAGING" "$OUT"
    STAGING=
    rm -rf "$PREVIOUS"
}
