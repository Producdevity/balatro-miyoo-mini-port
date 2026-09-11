# Architecture

The runtime implements the LOVE API that Balatro uses. It runs the original Lua
game code with in-memory changes for the Mini's screen, controls and game loop.
The game archive stays unchanged. The LOVE bindings and scalar CPU renderer
started as a fork of balatro-port-tui. The Mini backend adds framebuffer output,
evdev input, audio, ARM kernels and a threaded render pipeline.

## Components

- `crates/runtime`: startup, game loop, input, framebuffer output and replays.
- `crates/love-api`: Lua bindings, resources, audio and game-specific patches.
- `crates/renderer`: CPU rasterization and ARM NEON kernels.
- `native/onion-audio.c`: the helper that connects to Onion's audio server.
- `port`: the Onion launcher, shortcut and installation instructions.

ARM builds link Rust, musl and LuaJIT statically. LuaJIT runs in interpreter
mode; tested JIT configurations used more memory without a consistent frame-time
benefit. Host tests use Lua 5.1. The separate audio helper links against glibc
because Onion's installed audio wrapper cannot be loaded into the musl runtime.

## Rendering

The Mini renders at 640x480. Images use the complete transform stack before
entering an axis-aligned or affine raster path. Card effects run on the CPU.
The desktop CRT, bloom and shadow passes are disabled; full visual parity with
desktop Balatro has not been established.

The game thread submits ordered raster jobs. Jobs own the pixels they need,
so collecting a Lua resource cannot free an image still in use. A bounded
frame pipeline overlaps game updates and rendering. Large animated card draws
can split disjoint rows between the raster thread and a persistent helper.
Presentation retains frame order and applies framebuffer channel order and
rotation before page flipping where supported.

Optimizations preserve fallbacks for clipping, stencils and unsupported blends:

- Nearest-neighbor sprites combine sampling, tinting and blending in NEON.
- Adjacent compatible card layers skip shading base pixels covered by the face.
- Played and negative effects reuse prepared source colours in a cache capped
  at 512 KiB and 32 entries, including queued images.
- Scoring flames reuse turbulence on the shader's existing grid, while edges
  and colour gradients still use each output pixel.
- Text objects share immutable rasters through a weak-reference table capped
  at 512 keys. Removing a menu releases its owned text.

Scalar implementations remain available for comparison. Pixel tests cover the
optimized paths; changing operation order can affect rounding and alpha blending.

## Game integration

`game_source.rs` indexes the ZIP archive and decompresses requested files.
`miyoo/patches.rs` applies the load-time changes. Layout, movement, popups,
music and menu clocks have separate Lua modules. The compatibility API covers
what this game needs; it is not a complete LOVE implementation.

Movement shortcuts may skip settled base Moveables, but still call subclass
methods such as CardArea's alignment update. UI callbacks read the preceding
controller pass's collision state before it resets. Changing that order breaks
tooltip lifetime.

Saves are written synchronously through temporary files in Onion's save
directory. The launcher restores its clock changes on normal exit and handled
signals. It leaves swap and zram configuration alone.

## Controls

`runtime/src/platform/input.rs` reads evdev events. `miyoo_input.rs` and
`miyoo_input.lua` map them to gamepad callbacks, preserving press/release order.
Several D-pad taps received during one slow frame can move focus in that
update. Other actions are limited to one press per update to preserve the
order of selection, play, discard and menu changes.

A held direction repeats after 0.3 seconds, then every 0.1 seconds. Separate
taps act immediately. Releasing and pressing again resets the hold timer,
including when both events arrive between updates.

The adapter queues input through menu-opening locks for up to half a second.
Slow rendering alone does not expire queued input. Gameplay locks still block
actions during scoring. Controller taps bypass the option-arrow click debounce;
mouse clicks and disabled-button checks retain the game's behaviour.

## Layout

`love-api/src/miyoo/small_screen.lua` places the HUD, hand and actions in one
640x480 room. Played cards occupy a separate row. Text rescaling rebuilds
glyph metrics while retaining width limits, including undiscovered-card titles.

`popups.lua` fits tooltips above, below or beside the focused card.
`deck_layout.lua` draws the held deck overview above Jokers and suppresses
card tooltips until it closes. Paused-menu events use elapsed time; gameplay
events keep their original clock and pause behaviour.

## Regression checks

Host tests cover event order, held repeats, layout calculations and menu clocks.
Set `BALATRO_TEST_GAME` to include tests against the original game code.
See [device setup](../CONTRIBUTING.md#device-tests) before running these replays:

```sh
CONTROLS_TEST=rapid-input scripts/test-sp.sh
INPUT_FIXTURE=rapid-input TEST_FRAMES=900 WAIT_SECONDS=120 scripts/test-sp.sh
CONTROLS_TEST=layout TEST_FRAMES=3000 WAIT_SECONDS=240 scripts/test-sp.sh
CONTROLS_TEST=scoring AUTOPLAY_PAYOUT_JOKERS=2 \
  AUTOPLAY_STRESS_EFFECTS=1 TEST_FRAMES=1800 scripts/test-sp.sh
CONTROLS_TEST=blind TEST_FRAMES=1000 WAIT_SECONDS=120 scripts/test-sp.sh
```

The full controller replay in CONTRIBUTING.md also checks selection, play,
discard, menus and deck tabs. Inspect its captures for overlaps and focus
visibility. Physical button behaviour and latency need a Mini test.

See [audio](audio.md) and [performance testing](performance.md) for those checks.
