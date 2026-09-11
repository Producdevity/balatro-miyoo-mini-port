#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
PACKAGE=${1:?usage: test-deploy.sh RUNTIME_ONLY_PACKAGE}
PACKAGE=$(CDPATH='' cd -- "$PACKAGE" && pwd)
"$ROOT/scripts/verify-package.sh" "$PACKAGE" runtime-only
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for name in Balatro Balatro.exe BALATRO.LOVE empty; do
    sd=$WORK/$name
    game=$sd/Roms/PORTS/Games/Balatro
    save=$sd/Saves/CurrentProfile/saves/Balatro
    shortcuts=$sd/Roms/PORTS/Shortcuts/Strategy
    mkdir -p "$sd/Emu/PORTS" "$game/audio-cache" "$save" "$shortcuts"
    printf 'GameDataFile="Balatro"\n' >"$shortcuts/Balatro.notfound"
    printf 'old active shortcut\n' >"$shortcuts/Balatro.port"
    printf '#!/bin/sh\n' >"$sd/Emu/PORTS/launch_standalone.sh"
    printf 'save data\n' >"$save/profile.jkr"
    printf 'existing audio\n' >"$game/audio-cache/test.pcm"
    if [ "$name" != empty ]; then
        printf 'owned game\n' >"$game/$name"
    fi
    PACKAGE="$PACKAGE" ARCHIVE_ROOT="$sd/archives" \
        "$ROOT/scripts/deploy-miyoo.sh" "$sd"
    [ "$(cat "$save/profile.jkr")" = 'save data' ]
    [ "$(cat "$game/audio-cache/test.pcm")" = 'existing audio' ]
    if [ "$name" != empty ]; then
        [ "$(cat "$game/$name")" = 'owned game' ]
    fi
    cmp "$PACKAGE/Roms/PORTS/Games/Balatro/balatro-runtime" "$game/balatro-runtime"
    [ -x "$shortcuts/Balatro.notfound" ]
    [ ! -e "$shortcuts/Balatro.port" ]
    cmp "$PACKAGE/Roms/PORTS/Shortcuts/Strategy/Balatro.notfound" "$shortcuts/Balatro.notfound"
done
echo 'SD deployment tests passed'
