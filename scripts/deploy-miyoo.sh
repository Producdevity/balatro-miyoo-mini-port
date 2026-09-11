#!/bin/sh
set -eu
export COPYFILE_DISABLE=1

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
PACKAGE=${PACKAGE:-$ROOT/artifacts/balatro-miyoo}
SD_ROOT=${SD_ROOT:-${1:-/Volumes/MIYOO}}
ARCHIVE_ROOT=${ARCHIVE_ROOT:-/tmp}
SOURCE_GAME=$PACKAGE/Roms/PORTS/Games/Balatro
TARGET_GAMES=$SD_ROOT/Roms/PORTS/Games
TARGET_GAME=$TARGET_GAMES/Balatro
SAVE_DIR=$SD_ROOT/Saves/CurrentProfile/saves/Balatro
STAGE=$TARGET_GAMES/.Balatro.new.$$
OLD=$TARGET_GAMES/.Balatro.old.$$
ARCHIVE=$ARCHIVE_ROOT/balatro-miyoo-deploy-$(date +%Y%m%d-%H%M%S)
HAD_GAME=0

[ -d "$PACKAGE" ] || {
    echo "Package not found: $PACKAGE" >&2
    echo "Build it with scripts/package.sh before deploying." >&2
    exit 1
}

save_manifest() {
    output=$1
    if [ -d "$SAVE_DIR" ]; then
        find "$SAVE_DIR" -type f -exec shasum -a 256 {} + |
            sed "s#$SD_ROOT/##" |
            LC_ALL=C sort >"$output"
    else
        : >"$output"
    fi
}

game_files() {
    for path in "$1"/*; do
        [ -f "$path" ] || continue
        case "$(basename -- "$path" | tr '[:upper:]' '[:lower:]')" in
            balatro|balatro.exe|balatro.love) printf '%s\n' "$path" ;;
        esac
    done
}

game_hash() {
    game_files "$TARGET_GAME" | while IFS= read -r path; do
        shasum -a 256 "$path"
    done >"$1"
}

cleanup() {
    rm -rf "$STAGE"
    if [ -e "$OLD" ] && [ ! -e "$TARGET_GAME" ]; then
        mv "$OLD" "$TARGET_GAME"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

CONTENTS=runtime-only
[ ! -f "$SOURCE_GAME/Balatro" ] || CONTENTS=with-game
"$ROOT/scripts/verify-package.sh" "$PACKAGE" "$CONTENTS"
[ -d "$SD_ROOT/Emu/PORTS" ] || {
    echo "OnionOS SD card not found at $SD_ROOT" >&2
    exit 1
}
[ -f "$SD_ROOT/Emu/PORTS/launch_standalone.sh" ] || {
    echo "OnionOS standalone launcher is missing" >&2
    exit 1
}

mkdir -p "$ARCHIVE"
save_manifest "$ARCHIVE/saves-before.sha256"
game_hash "$ARCHIVE/game-before.sha256"
[ ! -s "$ARCHIVE/game-before.sha256" ] || HAD_GAME=1
if [ -d "$SAVE_DIR" ]; then
    tar -czf "$ARCHIVE/saves.tar.gz" -C "$SD_ROOT" "Saves/CurrentProfile/saves/Balatro"
fi
if [ -f "$SAVE_DIR/runtime.log" ]; then
    cp "$SAVE_DIR/runtime.log" "$ARCHIVE/runtime.log"
fi
if [ -f "$TARGET_GAME/balatro-runtime" ]; then
    shasum -a 256 "$TARGET_GAME/balatro-runtime" >"$ARCHIVE/previous-runtime.sha256"
fi
shasum -a 256 "$SOURCE_GAME/balatro-runtime" >"$ARCHIVE/deployed-runtime.sha256"

mkdir -p "$TARGET_GAMES" "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy" "$SD_ROOT/Roms/PORTS/Imgs"
cp -R "$SOURCE_GAME" "$STAGE"
find "$STAGE" -type f -name '._*' -delete
if [ "$HAD_GAME" = 1 ]; then
    game_files "$STAGE" | while IFS= read -r path; do rm "$path"; done
    game_files "$TARGET_GAME" | while IFS= read -r path; do
        cp "$path" "$STAGE/"
    done
fi
if [ ! -d "$SOURCE_GAME/audio-cache" ] && [ -d "$TARGET_GAME/audio-cache" ]; then
    cp -R "$TARGET_GAME/audio-cache" "$STAGE/audio-cache"
fi
cmp "$SOURCE_GAME/balatro-runtime" "$STAGE/balatro-runtime"
cmp "$SOURCE_GAME/balatro-audio" "$STAGE/balatro-audio"
cmp "$SOURCE_GAME/script.sh" "$STAGE/script.sh"
if [ -d "$SOURCE_GAME/audio-cache" ]; then
    diff -rq "$SOURCE_GAME/audio-cache" "$STAGE/audio-cache"
fi

if [ -e "$TARGET_GAME" ]; then
    mv "$TARGET_GAME" "$OLD"
fi
if ! mv "$STAGE" "$TARGET_GAME"; then
    [ ! -e "$OLD" ] || mv "$OLD" "$TARGET_GAME"
    exit 1
fi

cp "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound" \
    "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
chmod +x "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
# Onion promotes the new shortcut on import; discard the old active copy.
rm -f "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy/Balatro.port"
if [ -f "$PACKAGE/Roms/PORTS/Imgs/Balatro.png" ]; then
    cp "$PACKAGE/Roms/PORTS/Imgs/Balatro.png" "$SD_ROOT/Roms/PORTS/Imgs/Balatro.png"
fi

cmp "$SOURCE_GAME/balatro-runtime" "$TARGET_GAME/balatro-runtime"
cmp "$SOURCE_GAME/balatro-audio" "$TARGET_GAME/balatro-audio"
game_hash "$ARCHIVE/game-after.sha256"
if [ "$HAD_GAME" = 1 ]; then
    cmp "$ARCHIVE/game-before.sha256" "$ARCHIVE/game-after.sha256" || {
        echo "Game file verification failed" >&2
        exit 1
    }
fi
save_manifest "$ARCHIVE/saves-after.sha256"
cmp "$ARCHIVE/saves-before.sha256" "$ARCHIVE/saves-after.sha256" || {
    echo "Save verification failed; backup: $ARCHIVE/saves.tar.gz" >&2
    exit 1
}
rm -rf "$OLD"
rm -f \
    "$TARGET_GAMES/._Balatro" \
    "$TARGET_GAME/._Balatro" \
    "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy/._Balatro.port" \
    "$SD_ROOT/Roms/PORTS/Shortcuts/Strategy/._Balatro.notfound" \
    "$SD_ROOT/Roms/PORTS/Imgs/._Balatro.png"
trap - EXIT INT TERM

echo "Balatro installed at $TARGET_GAME"
[ "$HAD_GAME" != 1 ] || echo "Existing game file was not changed"
echo "Saves were not changed"
echo "Previous device state: $ARCHIVE"
