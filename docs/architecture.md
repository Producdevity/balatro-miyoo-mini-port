# Architecture

The runtime implements the LOVE API that Balatro uses. It runs the original Lua
game code with in-memory changes for the Mini's screen, controls and game loop.
The game archive stays unchanged. It does not run the desktop LOVE executable
or use the Stardew renderer.

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

Input comes from evdev. The adapter preserves press/release order and maps
buttons to gamepad actions. Saves are written synchronously through temporary
files in Onion's save directory. The launcher restores its clock changes on
normal exit and handled signals; it does not configure swap or zram.

See [controls](controls.md), [layout](layout.md), [audio](audio.md) and
[performance testing](performance.md) for the corresponding code and checks.
