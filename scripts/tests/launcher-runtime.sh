#!/bin/sh
set -eu
[ "$#" = 2 ] && [ "$1" = --onion ] && [ -d "$2" ]
printf '%s\n' "$$" >"$CLOCK_TEST_ROOT/game-pid"
case "${CLOCK_TEST_GAME:-normal}" in
    normal) exit 0 ;;
    failed) exit 7 ;;
    signal)
        trap 'exit 0' TERM
        while :; do sleep 0.05; done
        ;;
esac
