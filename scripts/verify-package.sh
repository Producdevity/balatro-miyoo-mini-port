#!/bin/sh
set -eu

PACKAGE=${1:?usage: verify-package.sh PACKAGE [with-game|runtime-only]}
CONTENTS=${2:-with-game}
GAME_DIR=$PACKAGE/Roms/PORTS/Games/Balatro

case "$CONTENTS" in
    with-game|runtime-only) ;;
    *) echo "unknown package type: $CONTENTS" >&2; exit 1 ;;
esac

for file in \
    "$GAME_DIR/balatro-runtime" \
    "$GAME_DIR/balatro-audio" \
    "$GAME_DIR/script.sh" \
    "$GAME_DIR/NOTICE" \
    "$GAME_DIR/licenses/runtime-GPL-3.0.txt" \
    "$GAME_DIR/licenses/balatro-port-tui.txt" \
    "$GAME_DIR/licenses/LuaJIT.txt" \
    "$GAME_DIR/licenses/Nunito.txt" \
    "$GAME_DIR/licenses/PortMaster.txt" \
    "$GAME_DIR/licenses/SLEEF.txt" \
    "$GAME_DIR/licenses/Rust.txt" \
    "$GAME_DIR/licenses/Rust-standard-library.html" \
    "$GAME_DIR/licenses/musl.txt" \
    "$GAME_DIR/licenses/GCC-exception-3.1.txt" \
    "$GAME_DIR/licenses/GPL-3.0-or-later.txt" \
    "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
do
    [ -s "$file" ] || { echo "missing package file: $file" >&2; exit 1; }
done

if [ "$CONTENTS" = with-game ]; then
    [ -s "$GAME_DIR/Balatro" ] || {
        echo "missing package file: $GAME_DIR/Balatro" >&2
        exit 1
    }
else
    if find "$PACKAGE" -type l | grep -q .; then
        echo "runtime-only package contains symlinks" >&2
        exit 1
    fi
    [ ! -e "$GAME_DIR/Balatro" ] || {
        echo "runtime-only package contains Balatro game data" >&2
        exit 1
    }
    if find "$PACKAGE" -type f \( -iname '*.love' -o -iname 'Balatro.exe' -o -iname '*.pcm' \) | grep -q .; then
        echo "runtime-only package contains Balatro game data" >&2
        exit 1
    fi
    find "$PACKAGE" -type f | while IFS= read -r path; do
        case "${path#"$PACKAGE"/}" in
            README.txt|Roms/PORTS/Shortcuts/Strategy/Balatro.notfound|\
            Roms/PORTS/Games/Balatro/balatro-runtime|\
            Roms/PORTS/Games/Balatro/balatro-audio|\
            Roms/PORTS/Games/Balatro/script.sh|\
            Roms/PORTS/Games/Balatro/NOTICE|\
            Roms/PORTS/Games/Balatro/licenses/*.txt|\
            Roms/PORTS/Games/Balatro/licenses/Rust-standard-library.html) ;;
            *) echo "Unexpected release file: $path" >&2; exit 1 ;;
        esac
    done
fi

file "$GAME_DIR/balatro-runtime" | grep -q 'ELF 32-bit.*ARM' || {
    echo "runtime is not an ARMv7 executable" >&2
    exit 1
}
file "$GAME_DIR/balatro-runtime" | grep -q 'statically linked' || {
    echo "runtime is not statically linked" >&2
    exit 1
}
file "$GAME_DIR/balatro-audio" | grep -q 'ELF 32-bit.*ARM.*dynamically linked.*interpreter /lib/ld-linux-armhf.so.3' || {
    echo "audio helper does not match the Onion ARM runtime" >&2
    exit 1
}
if [ "$CONTENTS" = with-game ]; then
    unzip -p "$GAME_DIR/Balatro" main.lua 2>/dev/null | grep -q 'require "game"' || {
        echo "game archive is invalid" >&2
        exit 1
    }
fi
grep -q 'BALATRO_PLATFORM=miyoo' "$GAME_DIR/script.sh"
# These are literal launcher assignments, including their shell defaults.
# shellcheck disable=SC2016
expected_rotate='export BALATRO_ROTATE_180="${BALATRO_ROTATE_180:-1}"'
# shellcheck disable=SC2016
expected_input='export BALATRO_INPUT_DEVICE="${BALATRO_INPUT_DEVICE:-/dev/input/event0}"'
# shellcheck disable=SC2016
expected_audio='export BALATRO_AUDIO="${BALATRO_AUDIO:-1}"'
grep -Fq "$expected_rotate" "$GAME_DIR/script.sh"
grep -Fq "$expected_input" "$GAME_DIR/script.sh"
grep -Fq "$expected_audio" "$GAME_DIR/script.sh"
grep -q 'launch_standalone.sh' "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
grep -q '^KillAudioserver=0$' "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
sh "$(dirname -- "$0")/test-shortcut.sh" "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound"
strings "$GAME_DIR/balatro-runtime" | grep -q '\[audio\] 44100 Hz stereo'
strings "$GAME_DIR/balatro-runtime" | grep -q '\[ui\] normalized invalid colour type='
if find "$GAME_DIR" -type f -name '._*' | grep -q . ||
    [ -e "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/._Balatro.notfound" ] ||
    [ -e "$PACKAGE/Roms/PORTS/Imgs/._Balatro.png" ]
then
    echo "package contains macOS metadata files" >&2
    exit 1
fi
echo "Package verified"
