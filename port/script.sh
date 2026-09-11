#!/bin/sh
set -u

GAME_DIR=$(dirname -- "$0")
GAME_DIR=$(CDPATH='' cd -- "$GAME_DIR" && pwd)
SYSTEM_DIR=${BALATRO_SYSTEM_DIR:-/mnt/SDCARD/.tmp_update}
SAVE_DIR=${BALATRO_SAVE_DIR:-/mnt/SDCARD/Saves/CurrentProfile/saves/Balatro}
CPU_CLOCK=$SYSTEM_DIR/bin/cpuclock
CPU_GOVERNOR=${BALATRO_CPU_GOVERNOR_FILE:-/sys/devices/system/cpu/cpufreq/policy0/scaling_governor}
CPU_INITIAL=
GOVERNOR_INITIAL=
CPU_CHANGED=0
GAME_PID=

mkdir -p "$SAVE_DIR" || exit 1
exec 2>"$SAVE_DIR/launcher.log"

# Invoked by the trap below.
# shellcheck disable=SC2329
finish() {
    result=$?
    trap - EXIT INT TERM
    if [ -n "$GAME_PID" ]; then
        kill -TERM "$GAME_PID" 2>/dev/null || true
        wait "$GAME_PID" 2>/dev/null || true
    fi
    if [ "$CPU_CHANGED" = 1 ]; then
        if restored=$("$CPU_CLOCK" "$CPU_INITIAL"); then
            echo "Clock restored: $restored MHz (was $CPU_INITIAL MHz)" >&2
        else
            echo 'Could not restore the CPU clock' >&2
        fi
        if printf '%s\n' "$GOVERNOR_INITIAL" >"$CPU_GOVERNOR"; then
            echo "Governor restored: $GOVERNOR_INITIAL" >&2
        else
            echo 'Could not restore the CPU governor' >&2
        fi
    fi
    echo "Game exited: $result" >&2
    exit "$result"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

set_clock() {
    target=${BALATRO_CPU_MHZ:-1500}
    [ "$target" != off ] || return 0
    case "$target" in
        ''|*[!0-9]*) echo 'BALATRO_CPU_MHZ must be off or 1200-1600' >&2; return 1 ;;
    esac
    if [ "$target" -lt 1200 ] || [ "$target" -gt 1600 ]; then
        echo 'BALATRO_CPU_MHZ must be off or 1200-1600' >&2
        return 1
    fi
    if [ ! -x "$CPU_CLOCK" ] || [ ! -r "$CPU_GOVERNOR" ] || [ ! -w "$CPU_GOVERNOR" ]; then
        echo 'Clock unchanged: Onion clock controls unavailable' >&2
        return 0
    fi
    if ! CPU_INITIAL=$("$CPU_CLOCK") || ! GOVERNOR_INITIAL=$(cat "$CPU_GOVERNOR"); then
        echo 'Clock unchanged: could not read the original settings' >&2
        return 0
    fi
    case "$CPU_INITIAL:$GOVERNOR_INITIAL" in
        *[!0-9a-zA-Z_:-]*|:*|*:) echo 'Clock unchanged: invalid original settings' >&2; return 0 ;;
    esac
    case "$CPU_INITIAL" in
        *[!0-9]*) echo 'Clock unchanged: invalid original frequency' >&2; return 0 ;;
    esac
    CPU_CHANGED=1
    if reported=$("$CPU_CLOCK" "$target"); then
        echo "Clock: $reported MHz (requested $target, original $CPU_INITIAL)" >&2
    else
        echo 'Could not apply the requested CPU clock' >&2
    fi
}
set_clock || exit 1

export BALATRO_PLATFORM=miyoo
export BALATRO_RENDER_WIDTH="${BALATRO_RENDER_WIDTH:-640}"
export BALATRO_RENDER_HEIGHT="${BALATRO_RENDER_HEIGHT:-480}"
export BALATRO_SHADER_COLOUR_CACHE="${BALATRO_SHADER_COLOUR_CACHE:-1}"
export BALATRO_SHADER_SPATIAL_CACHE="${BALATRO_SHADER_SPATIAL_CACHE:-0}"
export BALATRO_CARD_OCCLUSION="${BALATRO_CARD_OCCLUSION:-1}"
export BALATRO_LAYER_PAIRS="${BALATRO_LAYER_PAIRS:-1}"
export BALATRO_FLAME_SIMD="${BALATRO_FLAME_SIMD:-1}"
export BALATRO_FUSED_SHADER="${BALATRO_FUSED_SHADER:-1}"
export BALATRO_PACKED_PIXELS="${BALATRO_PACKED_PIXELS:-1}"
export BALATRO_FRAME_PIPELINE="${BALATRO_FRAME_PIPELINE:-2}"
export BALATRO_SHADER_WORKER="${BALATRO_SHADER_WORKER:-1}"
export BALATRO_DIRECT_PRESENT="${BALATRO_DIRECT_PRESENT:-1}"
export BALATRO_ROTATE_180="${BALATRO_ROTATE_180:-1}"
export BALATRO_INPUT_DEVICE="${BALATRO_INPUT_DEVICE:-/dev/input/event0}"
export BALATRO_AUDIO="${BALATRO_AUDIO:-1}"
export BALATRO_AUDIO_PRIORITY="${BALATRO_AUDIO_PRIORITY:-realtime}"
export BALATRO_PCM_CACHE="${BALATRO_PCM_CACHE:-$GAME_DIR/audio-cache}"
export BALATRO_SAVE_DIR="$SAVE_DIR"
export BALATRO_LOG="$SAVE_DIR/runtime.log"
export TUI_RENDER=framebuffer

cd "$GAME_DIR" || exit 1
./balatro-runtime --onion "$GAME_DIR" &
GAME_PID=$!
wait "$GAME_PID"
result=$?
GAME_PID=
exit "$result"
