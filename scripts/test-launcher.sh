#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
WORK=$(mktemp -d)
PID=
cleanup() {
    if [ -n "$PID" ]; then kill -TERM "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi
    rm -rf "$WORK"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export CLOCK_TEST_ROOT="$WORK"
export BALATRO_SYSTEM_DIR="$WORK/system"
export BALATRO_SAVE_DIR="$WORK/saves"
export BALATRO_CPU_GOVERNOR_FILE="$WORK/governor"
mkdir -p "$WORK/game" "$WORK/system/bin"
cp "$ROOT/port/script.sh" "$WORK/game/script.sh"
cp "$ROOT/scripts/tests/clock-helper.sh" "$WORK/system/bin/cpuclock"
cp "$ROOT/scripts/tests/launcher-runtime.sh" "$WORK/game/balatro-runtime"
chmod +x "$WORK/system/bin/cpuclock" "$WORK/game/balatro-runtime"

reset_case() {
    printf '1200\n' >"$WORK/frequency"
    printf 'ondemand\n' >"$WORK/governor"
    : >"$WORK/changes"
    rm -f "$WORK/game-pid"
}

check_restored() {
    [ "$(cat "$WORK/frequency")" = 1200 ]
    [ "$(cat "$WORK/governor")" = ondemand ]
    [ "$(cat "$WORK/changes")" = "$(printf '1500\n1200')" ]
}

reset_case
sh "$WORK/game/script.sh"
check_restored

reset_case
result=0
CLOCK_TEST_GAME=failed sh "$WORK/game/script.sh" || result=$?
[ "$result" = 7 ]
check_restored

reset_case
CLOCK_TEST_SET_FAIL=1 sh "$WORK/game/script.sh"
check_restored

for setting in off 2500 invalid; do
    reset_case
    result=0
    BALATRO_CPU_MHZ=$setting sh "$WORK/game/script.sh" || result=$?
    [ ! -s "$WORK/changes" ]
    if [ "$setting" = off ]; then [ "$result" = 0 ]; else [ "$result" = 1 ]; fi
done

reset_case
CLOCK_TEST_READ_FAIL=1 sh "$WORK/game/script.sh"
[ ! -s "$WORK/changes" ]

reset_case
CLOCK_TEST_GAME=signal sh "$WORK/game/script.sh" &
PID=$!
attempt=0
while [ ! -s "$WORK/game-pid" ]; do
    attempt=$((attempt + 1))
    [ "$attempt" -lt 100 ]
    sleep 0.05
done
kill -TERM "$PID"
result=0
wait "$PID" || result=$?
PID=
[ "$result" = 143 ]
check_restored
if kill -0 "$(cat "$WORK/game-pid")" 2>/dev/null; then exit 1; fi

echo 'Launcher clock and shutdown tests passed'
