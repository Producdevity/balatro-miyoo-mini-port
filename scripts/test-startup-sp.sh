#!/bin/sh
set -eu
SP_HOST=${SP_HOST:-muos-sp}
case "$SP_HOST" in ''|-*) echo 'Invalid SP_HOST' >&2; exit 1 ;; esac

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
PACKAGE=${PACKAGE:-$ROOT/artifacts/balatro-miyoo}
GAME=$PACKAGE/Roms/PORTS/Games/Balatro
CYCLES=${CYCLES:-30}
RESULTS=$ROOT/artifacts/startup-test/$(date +%Y%m%d-%H%M%S)
case "$CYCLES" in ''|*[!0-9]*|0) echo 'CYCLES must be positive' >&2; exit 1;; esac
[ "$CYCLES" -le 100 ] || { echo 'At most 100 startups per run' >&2; exit 1; }
"$ROOT/scripts/verify-package.sh" "$PACKAGE"
mkdir -p "$RESULTS"
REMOTE=$(ssh -o ConnectTimeout=8 "$SP_HOST" 'mktemp -d /tmp/balatro-startup.XXXXXX')
case "$REMOTE" in /tmp/balatro-startup.*) ;; *) exit 1;; esac
printf '%s\n' "$REMOTE" >"$RESULTS/remote.txt"
scp "$GAME/balatro-runtime" "$GAME/Balatro" "$SP_HOST:$REMOTE/" >/dev/null

# Headless initialization exercises Lua, game loading and audio without claiming
# frontend, input, display or performance coverage. Each launch has isolated saves.
# shellcheck disable=SC2029
ssh "$SP_HOST" "REMOTE='$REMOTE' CYCLES='$CYCLES' sh -s" <<'REMOTE_SCRIPT'
set -eu
cd "$REMOTE"
if pgrep -x balatro-runtime >/dev/null || pgrep -x retroarch >/dev/null; then
    echo 'A game is already running on the SP' >&2
    exit 1
fi
awk 'NR > 1 {found=1} END {exit found}' /proc/swaps || exit 1
ulimit -v 98304
ulimit -c 131072
sha256sum balatro-runtime Balatro >constraints.txt
printf 'cycles=%s memory_limit_kb=98304 render=640x480 audio=pcm\n' "$CYCLES" >>constraints.txt
cat /proc/swaps >>constraints.txt
cycle=0
while [ "$cycle" -lt "$CYCLES" ]; do
    cycle=$((cycle + 1))
    mkdir "save-$cycle"
    status=0
    BALATRO_PLATFORM=miyoo TUI_RENDER=headless \
    BALATRO_RENDER_WIDTH=640 BALATRO_RENDER_HEIGHT=480 \
    BALATRO_TEST_FRAMES=1 BALATRO_TEST_HOLD_MS=0 \
    BALATRO_AUDIO=1 BALATRO_AUDIO_CAPTURE="$REMOTE/audio.pcm" \
    BALATRO_AUDIO_PRIORITY=normal BALATRO_SAVE_DIR="$REMOTE/save-$cycle" \
    BALATRO_LOG="$REMOTE/start-$cycle.log" \
        timeout 30 taskset -c 0,1 ./balatro-runtime ./Balatro >"process-$cycle.log" 2>&1 || status=$?
    printf 'cycle=%s exit=%s\n' "$cycle" "$status" | tee -a results.txt
    if [ "$status" != 0 ] || ! grep -q 'test frame limit reached: 1' "start-$cycle.log"; then
        break
    fi
    rm -f audio.pcm
done
REMOTE_SCRIPT

# Keep a failing installation available for debugging until explicitly removed.
# shellcheck disable=SC2029
ssh "$SP_HOST" "cd '$REMOTE' && tar -cf - constraints.txt results.txt *.log" |
    tar -xf - -C "$RESULTS"
# shellcheck disable=SC2029
if ssh "$SP_HOST" "test -f '$REMOTE/core'"; then
    scp "$SP_HOST:$REMOTE/core" "$RESULTS/core" >/dev/null
fi
if [ "$(wc -l <"$RESULTS/results.txt" | tr -d ' ')" != "$CYCLES" ] ||
    grep -Eq 'exit=[1-9]' "$RESULTS/results.txt"; then
    echo "Startup failed; evidence: $RESULTS; device files: $REMOTE" >&2
    exit 1
fi
for log in "$RESULTS"/start-*.log; do
    grep -q 'test frame limit reached: 1' "$log" || exit 1
done
# shellcheck disable=SC2029
ssh "$SP_HOST" "rm -rf '$REMOTE'"
echo "Startups passed: $RESULTS"
