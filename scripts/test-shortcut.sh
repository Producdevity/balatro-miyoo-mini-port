#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
SHORTCUT=${1:-$ROOT/port/Balatro.notfound}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Onion's importer reads literal assignments, then checks one find -iname pattern.
# https://github.com/OnionUI/Onion/blob/main/static/packages/Emu/Ports%20Collection/Emu/PORTS/import.sh
field() {
    grep "$1=" "$SHORTCUT" | cut -d '=' -f2 | grep -o '".*"' | tr -d '"'
}

executable=$(field GameExecutable)
pattern=$(field GameDataFile)
[ -n "$pattern" ] || pattern=$executable
[ "$(field GameDir)" = Balatro ]
[ "$executable" = script.sh ]

for name in Balatro Balatro.exe Balatro.love BALATRO.LOVE missing; do
    game=$WORK/$name
    mkdir -p "$game"
    cp "$ROOT/port/script.sh" "$game/$executable"
    if [ "$name" != missing ]; then
        printf 'owned game\n' >"$game/$name"
    fi
    if ! find "$game" -maxdepth 2 -type f -iname "$pattern" | grep -q .; then
        echo "Onion hid the launcher with game file: $name" >&2
        exit 1
    fi
done

# A missing installation must still be hidden. A missing game is reported by setup.rs.
mkdir "$WORK/uninstalled"
if find "$WORK/uninstalled" -maxdepth 2 -type f -iname "$pattern" | grep -q .; then
    echo 'Onion accepted a missing installation' >&2
    exit 1
fi
echo 'Onion shortcut tests passed'
