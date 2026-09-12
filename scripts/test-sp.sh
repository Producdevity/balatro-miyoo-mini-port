#!/bin/sh
set -eu
SP_HOST=${SP_HOST:-muos-sp}
case "$SP_HOST" in ''|-*) echo 'Invalid SP_HOST' >&2; exit 1 ;; esac
export COPYFILE_DISABLE=1

ROOT=$(dirname -- "$0")
ROOT=$(CDPATH='' cd -- "$ROOT/.." && pwd)
PACKAGE=${PACKAGE:-$ROOT/artifacts/balatro-miyoo}
REMOTE=/tmp/balatro-miyoo-test
GAME_REL='Roms/PORTS/Games/Balatro/Balatro'
INSTALL_TEST=${INSTALL_TEST:-off}
case "$INSTALL_TEST" in
    off) ;;
    cold|warm) GAME_REL='Roms/PORTS/Games/Balatro/Balatro.exe' ;;
    *) echo 'INSTALL_TEST must be off, cold or warm' >&2; exit 1 ;;
esac
GAME_CACHE=/tmp/balatro-miyoo-game
RESULTS=$ROOT/artifacts/sp-test/$(date +%Y%m%d-%H%M%S)
TEST_FRAMES=${TEST_FRAMES:-720}
CAPTURE_DELAY=${CAPTURE_DELAY:-20}
TEST_HOLD_MS=${TEST_HOLD_MS:-3000}
WAIT_SECONDS=${WAIT_SECONDS:-60}
PROFILE=${PROFILE:-0}
NATIVE_PROFILE=${NATIVE_PROFILE:-0}
NATIVE_PROFILE_THREAD=${NATIVE_PROFILE_THREAD:-raster}
VERIFY_RASTER=${VERIFY_RASTER:-0}
SHADER_BATCH=${SHADER_BATCH:-1}
SHADER_NUMERIC=${SHADER_NUMERIC:-1}
NOISE_AXES=${NOISE_AXES:-1}
AFFINE_SPANS=${AFFINE_SPANS:-1}
NEON_SPRITES=${NEON_SPRITES:-1}
SPRITE_ALPHA=${SPRITE_ALPHA:-1}
FLAME_CACHE=${FLAME_CACHE:-1}
NATIVE_PRESENT=${NATIVE_PRESENT:-1}
DIRECT_PRESENT=${DIRECT_PRESENT:-1}
ROTATE_180=${ROTATE_180:-0}
ROW_FILL=${ROW_FILL:-1}
BULK_CLEAR=${BULK_CLEAR:-1}
OUTER_CLEAR=${OUTER_CLEAR:-0}
DEFER_FLAME=${DEFER_FLAME:-1}
GC_OVERLAP=${GC_OVERLAP:-1}
CARD_OCCLUSION=${CARD_OCCLUSION:-1}
FLAME_SIMD=${FLAME_SIMD:-1}
SPRITE_SPANS=${SPRITE_SPANS:-0}
FUSED_SHADER=${FUSED_SHADER:-1}
LAYER_ELISION=${LAYER_ELISION:-0}
FRAME_PIPELINE=${FRAME_PIPELINE:-2}
PACKED_PIXELS=${PACKED_PIXELS:-1}
SHADER_WORKER=${SHADER_WORKER:-1}
SHADER_WORKER_MIN_PIXELS=${SHADER_WORKER_MIN_PIXELS:-4096}
PREPARED_CARDS=${PREPARED_CARDS:-1}
PLAIN_AFFINE=${PLAIN_AFFINE:-1}
CORE_DUMP_KB=${CORE_DUMP_KB:-0}
PROFILE_SKIP_HUD=${PROFILE_SKIP_HUD:-0}
BENCHMARK=${BENCHMARK:-1}
AUTOPLAY_SETTLE_FRAMES=${AUTOPLAY_SETTLE_FRAMES:-60}
AUTOPLAY_MENU_INTERVAL=${AUTOPLAY_MENU_INTERVAL:-90}
AUTOPLAY_DELAY=${AUTOPLAY_DELAY:-120}
AUTOPLAY_HOLD_HAND=${AUTOPLAY_HOLD_HAND:-0}
AUTOPLAY_HOLD_SHOP=${AUTOPLAY_HOLD_SHOP:-0}
AUTOPLAY_HOLD_ROUND_EVAL=${AUTOPLAY_HOLD_ROUND_EVAL:-0}
AUTOPLAY_PAYOUT_JOKERS=${AUTOPLAY_PAYOUT_JOKERS:-0}
AUTOPLAY_STRESS_EFFECTS=${AUTOPLAY_STRESS_EFFECTS:-0}
MEMORY_LIMIT_KB=${MEMORY_LIMIT_KB:-98304}
CPUSET=${CPUSET:-0,1}
CPU_FREQ_KHZ=${CPU_FREQ_KHZ:-1512000}
JIT_SCOPE=${JIT_SCOPE:-off}
REDUCED_MOTION=${REDUCED_MOTION:-0}
STATIC_UI_CACHE=${STATIC_UI_CACHE:-1}
INPUT_FIXTURE=${INPUT_FIXTURE:-0}
LUA_SAMPLE_INTERVAL=${LUA_SAMPLE_INTERVAL:-0}
SCALAR_COLLISION=${SCALAR_COLLISION:-1}
SHADER_COLOUR_CACHE=${SHADER_COLOUR_CACHE:-1}
SHADER_SPATIAL_CACHE=${SHADER_SPATIAL_CACHE:-0}
RENDER_WIDTH=${RENDER_WIDTH:-640}
RENDER_HEIGHT=${RENDER_HEIGHT:-480}
CONTROLS_TEST=${CONTROLS_TEST:-0}
case "$CONTROLS_TEST" in
    1) AUTOPLAY_TEST_BLIND_CHIPS=${AUTOPLAY_TEST_BLIND_CHIPS:-0} ;;
    *) AUTOPLAY_TEST_BLIND_CHIPS=${AUTOPLAY_TEST_BLIND_CHIPS:-1} ;;
esac
case "$INPUT_FIXTURE" in
    0|1) ;;
    rapid-input) CONTROLS_TEST=evdev-input ;;
    *) echo 'INPUT_FIXTURE must be 0, 1 or rapid-input' >&2; exit 1 ;;
esac
AUDIO_CAPTURE=${AUDIO_CAPTURE:-1}
FETCH_AUDIO=${FETCH_AUDIO:-0}
AUDIO_PRIORITY=${AUDIO_PRIORITY:-realtime}
PCM_CACHE=${PCM_CACHE:-1}
OPACITY_CACHE_ENTRIES=${OPACITY_CACHE_ENTRIES:-256}
LAYER_PAIRS=${LAYER_PAIRS:-1}
GC_STEP_KIB=${GC_STEP_KIB:-128}
PREPARED_NEON=${PREPARED_NEON:-1}

case "$LAYER_PAIRS" in
    0|1) ;;
    *) echo 'LAYER_PAIRS must be 0 or 1' >&2; exit 1 ;;
esac
case "$GC_STEP_KIB" in
    32|64|128|256) ;;
    *) echo 'GC_STEP_KIB must be 32, 64, 128 or 256' >&2; exit 1 ;;
esac
case "$PREPARED_NEON" in
    0|1) ;;
    *) echo 'PREPARED_NEON must be 0 or 1' >&2; exit 1 ;;
esac
case "$NOISE_AXES:$FETCH_AUDIO" in
    [01]:[01]) ;;
    *) echo 'NOISE_AXES and FETCH_AUDIO must be 0 or 1' >&2; exit 1 ;;
esac

case "$OPACITY_CACHE_ENTRIES" in
    64|128|256|512) ;;
    *) echo 'OPACITY_CACHE_ENTRIES must be 64, 128, 256 or 512' >&2; exit 1 ;;
esac

case "$RENDER_WIDTH:$RENDER_HEIGHT" in
    320:240|640:480) ;;
    *) echo 'test resolution must be 320x240 or 640x480' >&2; exit 1 ;;
esac
case "$AUDIO_PRIORITY" in
    normal|realtime) ;;
    *) echo 'AUDIO_PRIORITY must be normal or realtime' >&2; exit 1 ;;
esac
case "$AUDIO_CAPTURE" in
    0|1) ;;
    *) echo 'AUDIO_CAPTURE must be 0 or 1' >&2; exit 1 ;;
esac
case "$PCM_CACHE" in
    0|1) ;;
    *) echo 'PCM_CACHE must be 0 or 1' >&2; exit 1 ;;
esac
case "$NATIVE_PROFILE_THREAD" in
    main|raster) ;;
    *) echo 'NATIVE_PROFILE_THREAD must be main or raster' >&2; exit 1 ;;
esac

case "$VERIFY_RASTER:$SHADER_BATCH:$PREPARED_CARDS:$PLAIN_AFFINE" in
    [01]:[01]:[01]:[01]) ;;
    *) echo 'Raster test switches must be 0 or 1' >&2; exit 1 ;;
esac
case "$SHADER_NUMERIC:$AFFINE_SPANS:$NEON_SPRITES:$SPRITE_ALPHA:$FLAME_CACHE:$NATIVE_PRESENT:$ROTATE_180:$ROW_FILL" in
    [01]:[01]:[01]:[01]:[01]:[01]:[01]:[01]) ;;
    *) echo 'Shader and sprite switches must be 0 or 1' >&2; exit 1 ;;
esac
case "$CORE_DUMP_KB" in
    ''|*[!0-9]*) echo 'CORE_DUMP_KB must be an integer' >&2; exit 1 ;;
esac
case "$BULK_CLEAR:$OUTER_CLEAR:$DEFER_FLAME:$GC_OVERLAP:$CARD_OCCLUSION:$FLAME_SIMD:$SPRITE_SPANS:$FUSED_SHADER:$LAYER_ELISION:$FRAME_PIPELINE" in
    [01]:[01]:[01]:[01]:[01]:[01]:[01]:[01]:[01]:[02]) ;;
    *) echo 'Frame switches must be 0 or 1; FRAME_PIPELINE accepts 0 or 2' >&2; exit 1 ;;
esac
case "$PACKED_PIXELS:$SHADER_WORKER:$DIRECT_PRESENT" in
    [01]:[01]:[01]) ;;
    *) echo 'Pixel switches must be 0 or 1' >&2; exit 1 ;;
esac
case "$SHADER_WORKER_MIN_PIXELS" in
    ''|*[!0-9]*) echo 'SHADER_WORKER_MIN_PIXELS must be an integer' >&2; exit 1 ;;
esac
if [ "$SHADER_WORKER_MIN_PIXELS" -lt 256 ] || [ "$SHADER_WORKER_MIN_PIXELS" -gt 65536 ]; then
    echo 'SHADER_WORKER_MIN_PIXELS must be between 256 and 65536' >&2
    exit 1
fi
[ "$CORE_DUMP_KB" -le 131072 ] || { echo 'Core dumps are limited to 128 MiB' >&2; exit 1; }

if [ "${SKIP_PACKAGE:-0}" != 1 ]; then
    "$ROOT/scripts/package.sh" >/dev/null
fi
mkdir -p "$RESULTS"
if [ "$INSTALL_TEST" != off ]; then
    "$ROOT/scripts/verify-package.sh" "$PACKAGE" runtime-only
fi
if [ "$INSTALL_TEST" = off ] && [ "$PCM_CACHE" = 1 ] && [ -d "$PACKAGE/${GAME_REL%/*}/audio-cache" ]; then
    rsync -rc "$PACKAGE/${GAME_REL%/*}/audio-cache/" \
        "$SP_HOST:/mnt/mmc/balatro-miyoo-audio-cache/"
fi

GAME_FILE=${GAME_FILE:-$PACKAGE/$GAME_REL}
if command -v sha256sum >/dev/null 2>&1; then
    GAME_HASH=$(sha256sum "$GAME_FILE" | awk '{print $1}')
else
    GAME_HASH=$(shasum -a 256 "$GAME_FILE" | awk '{print $1}')
fi

# These constants are deliberately expanded before the remote command runs.
# shellcheck disable=SC2029
if ! ssh "$SP_HOST" "test -f '$GAME_CACHE' && grep -qx '$GAME_HASH' '$GAME_CACHE.sha256'"; then
    scp "$GAME_FILE" "$SP_HOST:$GAME_CACHE.next"
    # shellcheck disable=SC2029
    ssh "$SP_HOST" "mv '$GAME_CACHE.next' '$GAME_CACHE' && printf '%s\n' '$GAME_HASH' > '$GAME_CACHE.sha256'"
fi

# shellcheck disable=SC2029
ssh "$SP_HOST" "rm -rf '$REMOTE' && mkdir -p '$REMOTE'"
# shellcheck disable=SC2029
tar -C "$PACKAGE" --exclude="./$GAME_REL" --exclude="./${GAME_REL%/*}/audio-cache" -cf - . |
    ssh "$SP_HOST" "tar -C '$REMOTE' -xf - && ln '$GAME_CACHE' '$REMOTE/$GAME_REL'"

# The capture marker enables the mixer without opening the SP audio device.
# shellcheck disable=SC2029
ssh "$SP_HOST" "echo '$INSTALL_TEST' > '$REMOTE/install-test'"
# shellcheck disable=SC2029
ssh "$SP_HOST" "printf '%s\\n' '$AUDIO_CAPTURE' '$AUDIO_PRIORITY' '$PCM_CACHE' > '$REMOTE/audio-capture'"
# shellcheck disable=SC2029
ssh "$SP_HOST" "printf '%s\\n' '$SHADER_NUMERIC' '$AFFINE_SPANS' '$NEON_SPRITES' '$SPRITE_ALPHA' '$FLAME_CACHE' '$NATIVE_PRESENT' '$ROTATE_180' '$ROW_FILL' '$BULK_CLEAR' '$OUTER_CLEAR' '$DEFER_FLAME' '$GC_OVERLAP' '$CARD_OCCLUSION' '$FLAME_SIMD' '$SPRITE_SPANS' '$FUSED_SHADER' '$LAYER_ELISION' '$FRAME_PIPELINE' '$PACKED_PIXELS' '$SHADER_WORKER' '$SHADER_WORKER_MIN_PIXELS' '$DIRECT_PRESENT' > '$REMOTE/raster-options'"
# shellcheck disable=SC2029
ssh "$SP_HOST" "printf '%s\\n' '$NATIVE_PROFILE_THREAD' > '$REMOTE/profile-thread'"
# shellcheck disable=SC2029
ssh "$SP_HOST" "printf '%s\\n' '$NOISE_AXES' > '$REMOTE/noise-axes'"
# shellcheck disable=SC2029
ssh "$SP_HOST" "printf '%s\\n' '$OPACITY_CACHE_ENTRIES' '$LAYER_PAIRS' '$GC_STEP_KIB' '$PREPARED_NEON' > '$REMOTE/cache-options'"

# Test settings are passed as quoted remote assignments.
# shellcheck disable=SC2029
ssh "$SP_HOST" "PLAIN_AFFINE='$PLAIN_AFFINE' CORE_DUMP_KB='$CORE_DUMP_KB' PREPARED_CARDS='$PREPARED_CARDS' SHADER_BATCH='$SHADER_BATCH' VERIFY_RASTER='$VERIFY_RASTER' TEST_FRAMES='$TEST_FRAMES' CAPTURE_DELAY='$CAPTURE_DELAY' TEST_HOLD_MS='$TEST_HOLD_MS' WAIT_SECONDS='$WAIT_SECONDS' PROFILE='$PROFILE' NATIVE_PROFILE='$NATIVE_PROFILE' PROFILE_SKIP_HUD='$PROFILE_SKIP_HUD' BENCHMARK='$BENCHMARK' AUTOPLAY_DELAY='$AUTOPLAY_DELAY' AUTOPLAY_SETTLE_FRAMES='$AUTOPLAY_SETTLE_FRAMES' AUTOPLAY_MENU_INTERVAL='$AUTOPLAY_MENU_INTERVAL' AUTOPLAY_HOLD_HAND='$AUTOPLAY_HOLD_HAND' AUTOPLAY_HOLD_SHOP='$AUTOPLAY_HOLD_SHOP' AUTOPLAY_HOLD_ROUND_EVAL='$AUTOPLAY_HOLD_ROUND_EVAL' AUTOPLAY_PAYOUT_JOKERS='$AUTOPLAY_PAYOUT_JOKERS' AUTOPLAY_TEST_BLIND_CHIPS='$AUTOPLAY_TEST_BLIND_CHIPS' AUTOPLAY_STRESS_EFFECTS='$AUTOPLAY_STRESS_EFFECTS' MEMORY_LIMIT_KB='$MEMORY_LIMIT_KB' CPUSET='$CPUSET' CPU_FREQ_KHZ='$CPU_FREQ_KHZ' JIT_SCOPE='$JIT_SCOPE' REDUCED_MOTION='$REDUCED_MOTION' STATIC_UI_CACHE='$STATIC_UI_CACHE' INPUT_FIXTURE='$INPUT_FIXTURE' LUA_SAMPLE_INTERVAL='$LUA_SAMPLE_INTERVAL' SCALAR_COLLISION='$SCALAR_COLLISION' SHADER_COLOUR_CACHE='$SHADER_COLOUR_CACHE' SHADER_SPATIAL_CACHE='$SHADER_SPATIAL_CACHE' RENDER_WIDTH='$RENDER_WIDTH' RENDER_HEIGHT='$RENDER_HEIGHT' CONTROLS_TEST='$CONTROLS_TEST' sh -s" <<'REMOTE_SCRIPT'
set -u
REMOTE=/tmp/balatro-miyoo-test
GAME="$REMOTE/Roms/PORTS/Games/Balatro"
SAVE="$REMOTE/save"
LOG="$REMOTE/runtime.log"
FRAME="$REMOTE/frame.png"
AUDIO_CAPTURE=$(sed -n '1p' "$REMOTE/audio-capture")
BALATRO_AUDIO_PRIORITY=$(sed -n '2p' "$REMOTE/audio-capture")
PCM_CACHE=$(sed -n '3p' "$REMOTE/audio-capture")
INSTALL_TEST=$(cat "$REMOTE/install-test")
GAME_NAME=Balatro
if [ "$INSTALL_TEST" != off ]; then
    GAME_NAME=Balatro.exe
    export BALATRO_PCM_CACHE=/mnt/mmc/balatro-miyoo-install-test-audio-cache
    if [ "$INSTALL_TEST" = cold ]; then
        rm -rf "$BALATRO_PCM_CACHE"
    fi
elif [ "$PCM_CACHE" = 1 ]; then
    export BALATRO_PCM_CACHE=/mnt/mmc/balatro-miyoo-audio-cache
    [ -d "$BALATRO_PCM_CACHE" ] || { echo 'Prepare the SP audio cache first' >&2; exit 1; }
fi
BALATRO_SHADER_NUMERIC=$(sed -n '1p' "$REMOTE/raster-options")
BALATRO_AFFINE_SPANS=$(sed -n '2p' "$REMOTE/raster-options")
BALATRO_NEON_SPRITES=$(sed -n '3p' "$REMOTE/raster-options")
BALATRO_SPRITE_ALPHA=$(sed -n '4p' "$REMOTE/raster-options")
BALATRO_FLAME_CACHE=$(sed -n '5p' "$REMOTE/raster-options")
export BALATRO_FLAME_CACHE
BALATRO_NATIVE_PRESENT=$(sed -n '6p' "$REMOTE/raster-options")
export BALATRO_NATIVE_PRESENT
BALATRO_ROTATE_180=$(sed -n '7p' "$REMOTE/raster-options")
export BALATRO_ROTATE_180
BALATRO_ROW_FILL=$(sed -n '8p' "$REMOTE/raster-options")
export BALATRO_ROW_FILL
BALATRO_BULK_CLEAR=$(sed -n '9p' "$REMOTE/raster-options")
BALATRO_OUTER_CLEAR=$(sed -n '10p' "$REMOTE/raster-options")
export BALATRO_BULK_CLEAR BALATRO_OUTER_CLEAR
BALATRO_DEFER_FLAME=$(sed -n '11p' "$REMOTE/raster-options")
export BALATRO_DEFER_FLAME
BALATRO_GC_OVERLAP=$(sed -n '12p' "$REMOTE/raster-options")
export BALATRO_GC_OVERLAP
BALATRO_CARD_OCCLUSION=$(sed -n '13p' "$REMOTE/raster-options")
export BALATRO_CARD_OCCLUSION
BALATRO_FLAME_SIMD=$(sed -n '14p' "$REMOTE/raster-options")
export BALATRO_FLAME_SIMD
BALATRO_SPRITE_SPANS=$(sed -n '15p' "$REMOTE/raster-options")
export BALATRO_SPRITE_SPANS
BALATRO_FUSED_SHADER=$(sed -n '16p' "$REMOTE/raster-options")
export BALATRO_FUSED_SHADER
BALATRO_LAYER_ELISION=$(sed -n '17p' "$REMOTE/raster-options")
export BALATRO_LAYER_ELISION
BALATRO_FRAME_PIPELINE=$(sed -n '18p' "$REMOTE/raster-options")
export BALATRO_FRAME_PIPELINE
BALATRO_PACKED_PIXELS=$(sed -n '19p' "$REMOTE/raster-options")
export BALATRO_PACKED_PIXELS
BALATRO_SHADER_WORKER=$(sed -n '20p' "$REMOTE/raster-options")
export BALATRO_SHADER_WORKER
BALATRO_SHADER_WORKER_MIN_PIXELS=$(sed -n '21p' "$REMOTE/raster-options")
export BALATRO_SHADER_WORKER_MIN_PIXELS
BALATRO_DIRECT_PRESENT=$(sed -n '22p' "$REMOTE/raster-options")
export BALATRO_DIRECT_PRESENT
BALATRO_NATIVE_PROFILE_THREAD=$(cat "$REMOTE/profile-thread")
export BALATRO_NATIVE_PROFILE_THREAD
BALATRO_NOISE_AXES=$(cat "$REMOTE/noise-axes")
export BALATRO_NOISE_AXES
BALATRO_OPACITY_CACHE_ENTRIES=$(sed -n '1p' "$REMOTE/cache-options")
BALATRO_LAYER_PAIRS=$(sed -n '2p' "$REMOTE/cache-options")
BALATRO_GC_STEP_KIB=$(sed -n '3p' "$REMOTE/cache-options")
BALATRO_PREPARED_NEON=$(sed -n '4p' "$REMOTE/cache-options")
export BALATRO_OPACITY_CACHE_ENTRIES BALATRO_LAYER_PAIRS BALATRO_GC_STEP_KIB
export BALATRO_PREPARED_NEON
export BALATRO_SHADER_NUMERIC BALATRO_AFFINE_SPANS BALATRO_NEON_SPRITES BALATRO_SPRITE_ALPHA
export BALATRO_AUDIO_PRIORITY
if [ "$AUDIO_CAPTURE" = 1 ]; then
    export BALATRO_AUDIO_CAPTURE="$REMOTE/audio.pcm"
    export BALATRO_AUDIO_STATS=1
fi
FRONTEND_PIDS=
SYSTEM_AUDIO_PIDS=
INPUT_WRITER_PID=
CPU_POLICY=/sys/devices/system/cpu/cpu0/cpufreq
CPU_GOVERNOR=

resume_frontend() {
    for pid in $FRONTEND_PIDS; do
        kill -CONT "$pid" 2>/dev/null || true
    done
}
cleanup() {
    [ -z "${GAME_PID:-}" ] || kill "$GAME_PID" 2>/dev/null || true
    [ -z "$INPUT_WRITER_PID" ] || kill "$INPUT_WRITER_PID" 2>/dev/null || true
    for pid in $SYSTEM_AUDIO_PIDS; do
        if [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = pipewire ]; then
            kill -CONT "$pid" 2>/dev/null || true
        fi
    done
    if [ -n "$CPU_GOVERNOR" ]; then
        printf '%s\n' "$CPU_MAX_KHZ" >"$CPU_POLICY/scaling_max_freq"
        printf '%s\n' "$CPU_GOVERNOR" >"$CPU_POLICY/scaling_governor"
    fi
    resume_frontend
}
trap cleanup EXIT INT TERM

awk 'NR > 1 {found=1} END {exit found}' /proc/swaps || {
    echo 'SP tests require swap and zram to be disabled' >&2
    exit 1
}
grep -qw "$CPU_FREQ_KHZ" "$CPU_POLICY/scaling_available_frequencies" || {
    echo "unsupported test CPU frequency: $CPU_FREQ_KHZ" >&2
    exit 1
}
CPU_MAX_KHZ=$(cat "$CPU_POLICY/scaling_max_freq")
CPU_GOVERNOR=$(cat "$CPU_POLICY/scaling_governor")
printf '%s\n' "$CPU_FREQ_KHZ" >"$CPU_POLICY/scaling_max_freq" || exit 1
printf '%s\n' performance >"$CPU_POLICY/scaling_governor" || exit 1

for pattern in \
    /opt/muos/script/mux/frontend.sh \
    /opt/muos/frontend/muxfrontend \
    /opt/muos/frontend/muhotkey \
    /opt/muos/script/mux/idle.sh
do
    for pid in $(pgrep -f "$pattern" 2>/dev/null || true); do
        kill -STOP "$pid" 2>/dev/null || continue
        FRONTEND_PIDS="$FRONTEND_PIDS $pid"
    done
done

# PCM tests do not use PipeWire. Its real-time data loop can otherwise compete
# with the constrained game cores. Suspend it, without changing volume/settings.
if [ "$AUDIO_CAPTURE" = 1 ]; then
    for pid in $(pidof pipewire 2>/dev/null || true); do
        state=$(awk '/^State:/ {print $2}' "/proc/$pid/status" 2>/dev/null)
        [ "$state" != T ] && [ "$state" != t ] || continue
        kill -STOP "$pid" 2>/dev/null || continue
        SYSTEM_AUDIO_PIDS="$SYSTEM_AUDIO_PIDS $pid"
    done
fi

mkdir -p "$SAVE"
cd "$GAME"
ulimit -c "$CORE_DUMP_KB"
INPUT_DEVICE=/dev/input/event1
AUTOPLAY=1
INPUT_TRACE=0
if [ "$INPUT_FIXTURE" != 0 ]; then
    INPUT_DEVICE="$REMOTE/input.events"
    rm -f "$INPUT_DEVICE"
    mkfifo "$INPUT_DEVICE"
    [ "$INPUT_FIXTURE" != 1 ] || AUTOPLAY=0
    INPUT_TRACE=1
fi
if [ "$MEMORY_LIMIT_KB" -gt 0 ]; then
    ulimit -v "$MEMORY_LIMIT_KB"
fi
command -v taskset >/dev/null 2>&1 || {
    echo "taskset is required for Miyoo-constrained SP tests" >&2
    exit 1
}
{
    echo "cpuset=$CPUSET"
    echo "verify_raster=$VERIFY_RASTER"
    echo "shader_batch=$SHADER_BATCH"
    echo "shader_numeric=$BALATRO_SHADER_NUMERIC affine_spans=$BALATRO_AFFINE_SPANS neon_sprites=$BALATRO_NEON_SPRITES sprite_alpha=$BALATRO_SPRITE_ALPHA flame_cache=$BALATRO_FLAME_CACHE native_present=$BALATRO_NATIVE_PRESENT row_fill=$BALATRO_ROW_FILL"
    echo "prepared_cards=$PREPARED_CARDS"
    echo "bulk_clear=$BALATRO_BULK_CLEAR outer_clear=$BALATRO_OUTER_CLEAR"
    echo "defer_flame=$BALATRO_DEFER_FLAME"
    echo "gc_overlap=$BALATRO_GC_OVERLAP"
    echo "card_occlusion=$BALATRO_CARD_OCCLUSION"
    echo "flame_simd=$BALATRO_FLAME_SIMD"
    echo "sprite_spans=$BALATRO_SPRITE_SPANS"
    echo "fused_shader=$BALATRO_FUSED_SHADER"
    echo "layer_elision=$BALATRO_LAYER_ELISION"
    echo "frame_pipeline=$BALATRO_FRAME_PIPELINE"
    echo "packed_pixels=$BALATRO_PACKED_PIXELS"
    echo "shader_worker=$BALATRO_SHADER_WORKER"
    echo "shader_worker_min_pixels=$BALATRO_SHADER_WORKER_MIN_PIXELS"
    echo "direct_present=$BALATRO_DIRECT_PRESENT"
    echo "noise_axes=$BALATRO_NOISE_AXES"
    echo "plain_affine=$PLAIN_AFFINE core_dump_kb=$CORE_DUMP_KB"
    echo "suspended_pipewire=$SYSTEM_AUDIO_PIDS"
    echo "cpu_freq_khz=$CPU_FREQ_KHZ"
    printf 'cpu_actual_khz='
    cat "$CPU_POLICY/scaling_cur_freq"
    echo "memory_limit_kb=$MEMORY_LIMIT_KB"
    echo "scalar_collision=$SCALAR_COLLISION"
    echo "shader_colour_cache=$SHADER_COLOUR_CACHE"
    echo "shader_spatial_cache=$SHADER_SPATIAL_CACHE"
    echo "jit_scope=$JIT_SCOPE"
    echo "profile=$PROFILE native_profile=$NATIVE_PROFILE native_profile_thread=$BALATRO_NATIVE_PROFILE_THREAD lua_sample_interval=$LUA_SAMPLE_INTERVAL"
    echo "opacity_cache_entries=$BALATRO_OPACITY_CACHE_ENTRIES"
    echo "layer_pairs=$BALATRO_LAYER_PAIRS"
    echo "gc_step_kib=$BALATRO_GC_STEP_KIB"
    echo "prepared_neon=$BALATRO_PREPARED_NEON"
    echo "frames=$TEST_FRAMES hold_hand=$AUTOPLAY_HOLD_HAND hold_shop=$AUTOPLAY_HOLD_SHOP stress_effects=$AUTOPLAY_STRESS_EFFECTS"
    echo "hold_round_eval=$AUTOPLAY_HOLD_ROUND_EVAL"
    echo "payout_jokers=$AUTOPLAY_PAYOUT_JOKERS"
    echo "render_size=${RENDER_WIDTH}x${RENDER_HEIGHT} controls_test=$CONTROLS_TEST rotate180=$BALATRO_ROTATE_180"
    echo "audio_capture=$AUDIO_CAPTURE audio_priority=$BALATRO_AUDIO_PRIORITY"
    echo "pcm_cache=$PCM_CACHE"
    echo "install_test=$INSTALL_TEST"
    sha256sum ./balatro-runtime "./$GAME_NAME"
    printf 'online_cpus='
    getconf _NPROCESSORS_ONLN
    printf 'mem_total_kb='
    awk '/^MemTotal:/ {print $2}' /proc/meminfo
    echo 'swaps:'
    cat /proc/swaps
} >"$REMOTE/constraints.txt"

run_balatro() {
    if [ "$INSTALL_TEST" != off ]; then
        exec taskset -c "$CPUSET" ./balatro-runtime --onion "$GAME"
    else
        exec taskset -c "$CPUSET" ./balatro-runtime ./Balatro
    fi
}
BALATRO_PLATFORM=miyoo \
BALATRO_PLAIN_AFFINE="$PLAIN_AFFINE" \
BALATRO_VERIFY_RASTER="$VERIFY_RASTER" \
BALATRO_SHADER_BATCH="$SHADER_BATCH" \
BALATRO_PREPARED_CARDS="$PREPARED_CARDS" \
BALATRO_INPUT_DEVICE="$INPUT_DEVICE" \
BALATRO_INPUT_TRACE="$INPUT_TRACE" \
BALATRO_AUDIO="$AUDIO_CAPTURE" \
BALATRO_RENDER_WIDTH="$RENDER_WIDTH" \
BALATRO_RENDER_HEIGHT="$RENDER_HEIGHT" \
BALATRO_TEST_CONTROLS="$CONTROLS_TEST" \
BALATRO_SAVE_DIR="$SAVE" \
BALATRO_LOG="$LOG" \
BALATRO_PROFILE="$PROFILE" \
BALATRO_NATIVE_PROFILE="$NATIVE_PROFILE" \
BALATRO_PROFILE_SKIP_HUD="$PROFILE_SKIP_HUD" \
BALATRO_BENCHMARK="$BENCHMARK" \
BALATRO_JIT_SCOPE="$JIT_SCOPE" \
BALATRO_REDUCED_MOTION="$REDUCED_MOTION" \
BALATRO_STATIC_UI_CACHE="$STATIC_UI_CACHE" \
BALATRO_LUA_SAMPLE_INTERVAL="$LUA_SAMPLE_INTERVAL" \
BALATRO_SCALAR_COLLISION="$SCALAR_COLLISION" \
BALATRO_SHADER_COLOUR_CACHE="$SHADER_COLOUR_CACHE" \
BALATRO_SHADER_SPATIAL_CACHE="$SHADER_SPATIAL_CACHE" \
BALATRO_TEST_FRAMES="$TEST_FRAMES" \
BALATRO_TEST_SNAPSHOT="$REMOTE/internal.ppm" \
BALATRO_TEST_HOLD_MS="$TEST_HOLD_MS" \
TUI_AUTOPLAY="$AUTOPLAY" \
TUI_AUTOPLAY_HOLD_ROUND_EVAL="$AUTOPLAY_HOLD_ROUND_EVAL" \
TUI_AUTOPLAY_PAYOUT_JOKERS="$AUTOPLAY_PAYOUT_JOKERS" \
TUI_AUTOPLAY_DELAY="$AUTOPLAY_DELAY" \
TUI_AUTOPLAY_SETTLE_FRAMES="$AUTOPLAY_SETTLE_FRAMES" \
TUI_AUTOPLAY_MENU_INTERVAL="$AUTOPLAY_MENU_INTERVAL" \
TUI_AUTOPLAY_HOLD_HAND="$AUTOPLAY_HOLD_HAND" \
TUI_AUTOPLAY_HOLD_SHOP="$AUTOPLAY_HOLD_SHOP" \
TUI_AUTOPLAY_TEST_BLIND_CHIPS="$AUTOPLAY_TEST_BLIND_CHIPS" \
TUI_AUTOPLAY_STRESS_EFFECTS="$AUTOPLAY_STRESS_EFFECTS" \
TUI_RENDER=framebuffer \
run_balatro >"$REMOTE/process.log" 2>&1 &
GAME_PID=$!

if [ "$INPUT_FIXTURE" != 0 ]; then
    (
        attempt=0
        while [ "$attempt" -lt "$WAIT_SECONDS" ]; do
            grep -q '\[input\] opened' "$LOG" 2>/dev/null && break
            sleep 1
            attempt=$((attempt + 1))
        done
        sleep 1

        emit_button() {
            printf '%b' "\\000\\000\\000\\000\\000\\000\\000\\000\\001\\000$1\\000$2\\000\\000\\000"
        }

        emit_key() {
            emit_button "$1" '\001'
            emit_button "$1" '\000'
        }

        wait_for_input() {
            count=0
            until grep -q "\[controls-test\] evdev $1" "$LOG"; do
                kill -0 "$GAME_PID" 2>/dev/null || return 1
                [ "$count" -lt "$((WAIT_SECONDS * 10))" ] || return 1
                sleep 0.1
                count=$((count + 1))
            done
        }

        if [ "$INPUT_FIXTURE" = rapid-input ]; then
            wait_for_input 'ready for taps'
            { emit_key '\151'; emit_key '\151'; } >"$INPUT_DEVICE"
            wait_for_input 'ready for hold'
            emit_button '\151' '\001' >"$INPUT_DEVICE"
            wait_for_input 'ready for repress'
            {
                emit_button '\151' '\000'
                emit_button '\151' '\001'
            } >"$INPUT_DEVICE"
            wait_for_input 'repeat observed'
            emit_button '\151' '\000' >"$INPUT_DEVICE"
            exit
        fi

        {
            emit_key '\147'
            emit_key '\154'
            emit_key '\151'
            emit_key '\152'
            emit_key '\071'
            emit_key '\035'
            emit_key '\052'
            emit_key '\070'
            emit_key '\022'
            emit_key '\024'
            emit_key '\017'
            emit_key '\016'
            emit_key '\141'
            emit_key '\034'
        } >"$INPUT_DEVICE"
    ) &
    INPUT_WRITER_PID=$!
fi

attempt=0
: >"$REMOTE/memory.txt"
: >"$REMOTE/audio-thread.txt"
if [ "$TEST_HOLD_MS" -gt 0 ]; then
    while kill -0 "$GAME_PID" 2>/dev/null && [ "$attempt" -lt "$WAIT_SECONDS" ]; do
        grep -q 'test frame ready' "$LOG" 2>/dev/null && break
        grep -E 'VmPeak|VmHWM|VmRSS|VmSwap|Threads' "/proc/$GAME_PID/status" >>"$REMOTE/memory.txt" 2>/dev/null || true
        if [ "$AUDIO_CAPTURE" = 1 ]; then
            for task in /proc/"$GAME_PID"/task/*; do
                if [ "$(cat "$task/comm" 2>/dev/null)" = balatro-audio ]; then
                    cat /proc/uptime "$task/stat" "$task/schedstat" "$task/wchan" "$task/syscall" >>"$REMOTE/audio-thread.txt" 2>/dev/null || true
                    printf '\n' >>"$REMOTE/audio-thread.txt"
                fi
            done
        fi
        sleep 1
        attempt=$((attempt + 1))
    done
else
    sleep "$CAPTURE_DELAY"
fi
grep -E 'VmPeak|VmHWM|VmRSS|VmSwap|Threads' "/proc/$GAME_PID/status" >>"$REMOTE/memory.txt" 2>/dev/null || true
# The SP framebuffer's unused alpha byte must not make the PNG transparent.
fbgrab -a "$FRAME" >/dev/null 2>&1 || true

attempt=0
while kill -0 "$GAME_PID" 2>/dev/null && [ "$attempt" -lt "$WAIT_SECONDS" ]; do
    sleep 1
    attempt=$((attempt + 1))
done
GAME_EXIT=0
if kill -0 "$GAME_PID" 2>/dev/null; then
    kill "$GAME_PID" 2>/dev/null || true
    GAME_EXIT=124
fi
wait "$GAME_PID" 2>/dev/null || GAME_EXIT=$?
printf '%s\n' "$GAME_EXIT" >"$REMOTE/exit-code.txt"
printf 'cpu_final_khz=' >>"$REMOTE/constraints.txt"
cat "$CPU_POLICY/scaling_cur_freq" >>"$REMOTE/constraints.txt"
GAME_PID=
[ -z "$INPUT_WRITER_PID" ] || wait "$INPUT_WRITER_PID" 2>/dev/null || true
INPUT_WRITER_PID=
resume_frontend
FRONTEND_PIDS=
REMOTE_SCRIPT

scp "$SP_HOST:$REMOTE/constraints.txt" "$RESULTS/constraints.txt"
scp "$SP_HOST:$REMOTE/exit-code.txt" "$RESULTS/exit-code.txt"
# Core files stay inside the disposable test installation.
# shellcheck disable=SC2029
if ssh "$SP_HOST" "test -f '$REMOTE/Roms/PORTS/Games/Balatro/core'"; then
    scp "$SP_HOST:$REMOTE/Roms/PORTS/Games/Balatro/core" "$RESULTS/core"
fi
# A loader or early runtime crash can happen before logging or audio starts.
for name in process.log runtime.log frame.png internal.ppm memory.txt; do
    # shellcheck disable=SC2029
    if ssh "$SP_HOST" "test -f '$REMOTE/$name'"; then
        scp "$SP_HOST:$REMOTE/$name" "$RESULTS/$name"
    fi
done
if [ "$CONTROLS_TEST" != 0 ]; then
    # shellcheck disable=SC2029
    if ssh "$SP_HOST" "ls '$REMOTE'/controls-*.ppm >/dev/null 2>&1"; then
        scp "$SP_HOST:$REMOTE/controls-*.ppm" "$RESULTS/"
    fi
fi
if [ "$(cat "$RESULTS/exit-code.txt")" != 0 ] ||
    ! grep -q "test frame limit reached: $TEST_FRAMES" "$RESULTS/runtime.log"; then
    echo "Balatro test failed or timed out: $RESULTS" >&2
    exit 1
fi
if [ "$AUDIO_CAPTURE" = 1 ]; then
    if [ "$FETCH_AUDIO" = 1 ]; then
        scp "$SP_HOST:$REMOTE/audio.pcm" "$RESULTS/audio.pcm"
    fi
    scp "$SP_HOST:$REMOTE/audio-thread.txt" "$RESULTS/audio-thread.txt"
    grep -q '\[audio\] silent PCM capture:' "$RESULTS/runtime.log" || {
        echo 'Audio capture did not start' >&2
        exit 1
    }
fi
if [ "$CONTROLS_TEST" != 0 ]; then
    grep -q '\[controls-test\] PASS' "$RESULTS/runtime.log" || {
        echo "Controller test did not finish: $RESULTS" >&2
        exit 1
    }
    if [ "$CONTROLS_TEST" = layout ]; then
        if ! grep -q 'PASS: main menu button bounds' "$RESULTS/runtime.log" ||
           ! grep -q 'PASS: shop tooltip placement and animated text metrics' "$RESULTS/runtime.log"; then
            echo "Menu and shop checks did not both finish: $RESULTS" >&2
            exit 1
        fi
    fi
    if [ "$CONTROLS_TEST" = scoring ]; then
        if ! grep -q 'PASS: hand and bottom action spacing' "$RESULTS/runtime.log" ||
           ! grep -q 'PASS: scoring cards remain below owned cards' "$RESULTS/runtime.log"; then
            echo "Hand and scoring checks did not both finish: $RESULTS" >&2
            exit 1
        fi
    fi
    if [ "$CONTROLS_TEST" = languages-play ]; then
        if ! grep -q 'PASS: all language menu transitions' "$RESULTS/runtime.log" ||
           ! grep -q 'PASS: translated round and shop' "$RESULTS/runtime.log"; then
            echo "Language and gameplay checks did not both finish: $RESULTS" >&2
            exit 1
        fi
    fi
fi
scp -r "$SP_HOST:$REMOTE/save" "$RESULTS/save"

if grep -Eq '^\[AUTO\].* error:' "$RESULTS/runtime.log"; then
    echo "Balatro autoplay failed: $RESULTS" >&2
    exit 1
fi

if [ "$VERIFY_RASTER" = 1 ] &&
    ! grep -Eq '^\[raster-check\] [1-9][0-9]* batches matched sequential rendering$' "$RESULTS/runtime.log"; then
    echo "Raster comparison did not finish: $RESULTS" >&2
    exit 1
fi

if [ "$INPUT_FIXTURE" = 1 ]; then
    while IFS=: read -r code key button; do
        grep -Fq "[input] code=$code " "$RESULTS/runtime.log" || {
            echo "missing evdev input code $code" >&2
            exit 1
        }
        grep -Fq "[input] lua press key=$key button=$button" "$RESULTS/runtime.log" || {
            echo "missing Balatro input mapping $key -> $button" >&2
            exit 1
        }
    done <<'INPUTS'
103:miyoo_up:dpup
108:miyoo_down:dpdown
105:miyoo_left:dpleft
106:miyoo_right:dpright
57:miyoo_a:a
29:miyoo_b:b
42:miyoo_x:x
56:miyoo_y:y
18:miyoo_l1:leftshoulder
20:miyoo_r1:rightshoulder
15:miyoo_l2:triggerleft
14:miyoo_r2:triggerright
97:miyoo_select:back
28:miyoo_start:start
INPUTS
    echo "Miyoo input fixture passed"
fi
printf '%s\n' "$RESULTS"
