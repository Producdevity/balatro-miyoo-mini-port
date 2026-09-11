#!/bin/sh
set -eu
if [ "$#" = 0 ]; then
    [ "${CLOCK_TEST_READ_FAIL:-0}" != 1 ] || exit 1
    cat "$CLOCK_TEST_ROOT/frequency"
else
    printf '%s\n' "$1" >>"$CLOCK_TEST_ROOT/changes"
    printf '%s\n' "$1" >"$CLOCK_TEST_ROOT/frequency"
    printf 'userspace\n' >"$CLOCK_TEST_ROOT/governor"
    if [ "${CLOCK_TEST_SET_FAIL:-0}" = 1 ] && [ "$1" = 1500 ]; then exit 1; fi
    printf '%s\n' "$1"
fi
