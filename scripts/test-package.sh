#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# shellcheck source=scripts/lib/package.sh
. "$ROOT/scripts/lib/package.sh"

OUT=$WORK/package
mkdir -p "$OUT/Roms/PORTS/Games/Balatro"
printf old >"$OUT/Roms/PORTS/Games/Balatro/balatro-runtime"

if (
    prepare_output
    printf incomplete >"$STAGING/incomplete"
    exit 1
); then
    echo 'Expected a failed preparation' >&2
    exit 1
fi
[ "$(cat "$OUT/Roms/PORTS/Games/Balatro/balatro-runtime")" = old ]

if (
    prepare_output
    mv "$OUT" "$PREVIOUS"
    exit 1
); then
    echo 'Expected an interrupted replacement' >&2
    exit 1
fi
[ "$(cat "$OUT/Roms/PORTS/Games/Balatro/balatro-runtime")" = old ]

(
    prepare_output
    mkdir -p "$STAGING/Roms/PORTS/Games/Balatro"
    printf new >"$STAGING/Roms/PORTS/Games/Balatro/balatro-runtime"
    install_output
)
[ "$(cat "$OUT/Roms/PORTS/Games/Balatro/balatro-runtime")" = new ]
[ "$(find "$WORK" -maxdepth 1 -type d | wc -l | tr -d ' ')" = 2 ]
ln -s "$OUT" "$WORK/link"

mkdir "$WORK/unrelated"
printf keep >"$WORK/unrelated/file"
if (OUT=$WORK/unrelated; prepare_output) 2>/dev/null; then exit 1; fi
[ "$(cat "$WORK/unrelated/file")" = keep ]
if (OUT=$WORK/link; prepare_output) 2>/dev/null; then exit 1; fi

echo 'Package replacement tests passed'
