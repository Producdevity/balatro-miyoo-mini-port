#!/bin/sh
set -eu
SP_HOST=${SP_HOST:-muos-sp}
case "$SP_HOST" in ''|-*) echo 'Invalid SP_HOST' >&2; exit 1 ;; esac
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT=$ROOT/target/audio-tests
mkdir -p "$OUT"
sh "$ROOT/scripts/build-audio.sh"
zig cc -target arm-linux-gnueabihf.2.23 -mcpu=cortex_a7 -O2 -shared -fPIC \
    -Wall -Wextra -Werror "$ROOT/native/tests/oss-output.c" -ldl -pthread -o "$OUT/oss-output.so"
REMOTE=$(ssh -o ConnectTimeout=8 "$SP_HOST" 'mktemp -d /tmp/balatro-audio.XXXXXX')
case "$REMOTE" in /tmp/balatro-audio.*) ;; *) exit 1;; esac
scp "$OUT/oss-output.so" "$ROOT/target/armv7-unknown-linux-musleabihf/release/balatro-audio" "$SP_HOST:$REMOTE/" >/dev/null
# This preload routes every DSP open to an anonymous pipe. It never opens hardware.
# shellcheck disable=SC2029
ssh "$SP_HOST" "cd '$REMOTE' && sh -s" <<'REMOTE_SCRIPT'
set -eu
dd if=/dev/urandom of=input.pcm bs=4096 count=64 2>/dev/null
timeout 10 env TEST_CAPTURE="$PWD/output.pcm" LD_PRELOAD="$PWD/oss-output.so" \
    ./balatro-audio <input.pcm 2>normal.log
cmp input.pcm output.pcm
if timeout 5 env TEST_REJECT=1 TEST_CAPTURE="$PWD/rejected.pcm" LD_PRELOAD="$PWD/oss-output.so" \
    ./balatro-audio <input.pcm 2>rejected.log; then
    echo 'Helper accepted a rejected format' >&2; exit 1
fi
test ! -s rejected.pcm
grep -q 'output rejected request' rejected.log
set +e
timeout 5 env TEST_CUTOFF=4096 TEST_CAPTURE="$PWD/interrupted.pcm" LD_PRELOAD="$PWD/oss-output.so" \
    ./balatro-audio <input.pcm 2>interrupted.log
status=$?
set -e
test "$status" -ne 0 && test "$status" -ne 124
test "$(wc -c < interrupted.pcm)" -eq 4096
grep -Eq 'output stopped accepting samples|write output' interrupted.log
timeout 10 env TEST_CAPTURE="$PWD/reconnected.pcm" LD_PRELOAD="$PWD/oss-output.so" \
    ./balatro-audio <input.pcm 2>reconnected.log
cmp input.pcm reconnected.pcm
echo 'Audio helper passed: exact PCM, format checks, reader loss and restart'
REMOTE_SCRIPT
# shellcheck disable=SC2029
ssh "$SP_HOST" "rm -rf '$REMOTE'"
