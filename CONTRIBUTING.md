# Development

Host tests need Rust, Cargo and a C compiler. They run without Balatro or a
handheld. Some regression tests read the original game code; these are ignored
unless you supply your own game archive.

Rust is pinned in `rust-toolchain.toml`. Package builds use Zig 0.16.0 and
cargo-zigbuild 0.23.4, matching CI and the included toolchain notices.

```sh
scripts/check.sh
BALATRO_TEST_GAME=/path/to/Balatro.exe scripts/check.sh
```

The check script runs rustfmt, the workspace tests, launcher tests and package
replacement tests. Install ShellCheck to include shell linting. GitHub Actions
runs the checks on Linux and macOS, without game files or device access.

## ARM build

The build currently uses Rust, Zig, cargo-zigbuild, Docker, curl, tar and make.
Packaging also uses zip, unzip and a SHA-256 utility. Development has been done
on macOS; Linux host tests run separately from physical handheld checks.

```sh
rustup target add armv7-unknown-linux-musleabihf
cargo install --locked cargo-zigbuild --version 0.23.4
scripts/build.sh
scripts/package.sh /path/to/Balatro.exe
```

Docker builds the static LuaJIT library using an ARMv7 musl toolchain. It needs
to run `linux/arm/v7` containers, including on non-ARM hosts. The Debian image
in `build-luajit.sh` runs only inside Docker. Zig builds the Rust runtime,
NEON code and small Onion audio helper.
The runtime links statically; the audio helper uses Onion's glibc audio wrapper.

Dependencies are locked by `Cargo.lock`; the SLEEF download is checksum-checked.
Build output, downloaded sources and audio caches stay under `target/` and
`artifacts/`. Do not commit game archives, extracted game code, saves, PCM files
or copied artwork.

After changing Rust dependencies, run `python3 scripts/collect-licenses.py`.
It refreshes the license texts included in binary packages from Cargo's locked
ARM dependency graph. Keep upstream copyright notices when moving or editing
derived code.

## Layout

- `crates/runtime`: launch, frame scheduling, device input and presentation.
- `crates/love-api`: Lua bindings, resources, audio and load-time game patches.
- `crates/renderer`: CPU rasterizer, pixel operations and ARM NEON kernels.
- `native`: Onion audio helper and its test endpoint.
- `port`: Onion launcher, shortcut and installation instructions.
- `scripts`: build, package, deployment and test commands.
- `docs`: architecture, layout, controls, audio and performance testing.

Every PR and push to `master` runs the host checks and builds a game-free ARM
package. Download `balatro-miyoo-mini` from the workflow's artifacts to test it.
These builds do not run the game or establish hardware compatibility.

`scripts/package-source.sh` archives the committed tree, locked Cargo sources
and SLEEF for distribution beside the binary. It does not include local changes.
The source archive builds with the same commands; Cargo reads its bundled
`vendor` directory. Rust, Zig, Docker and system build tools are still required.

Graphics bindings are grouped by resource type under `love-api/src/graphics`.
`game_source.rs` reads the game archive; `miyoo/patches.rs` applies the port's
changes in memory. Keep runtime state separate from game-specific patches.
Terminal helpers, compatibility scripts, diagnostic scripts and automated
replays have separate directories under `runtime/src`.

The shortcut ships as `Balatro.notfound`. Onion's Ports import replaces any
old active shortcut and renames the new one to `.port`. It checks for
`script.sh`; the runtime then finds and validates the user's game archive.
If the game is missing, the launcher stays visible and shows where to copy it.

## Device tests

`scripts/test-sp.sh` uses an RG35XX SP running muOS. Set `SP_HOST` to its SSH
hostname; it defaults to `muos-sp`. The runner temporarily takes ownership of
the frontend, so do not run it while another game is active.

```sh
SP_HOST=muos-sp CONTROLS_TEST=1 AUDIO_CAPTURE=1 \
  AUTOPLAY_TEST_BLIND_CHIPS=300 AUTOPLAY_PAYOUT_JOKERS=2 \
  AUTOPLAY_STRESS_EFFECTS=1 TEST_FRAMES=2000 WAIT_SECONDS=180 \
  scripts/test-sp.sh
```

Tests use two cores at 1.512 GHz, a 96 MiB address-space limit and no swap.
Audio goes to a paced PCM capture without opening the speaker. The runner
restores CPU settings, the frontend and PipeWire afterward; it does not change
volume. Results go to `artifacts/sp-test/`. Add `FETCH_AUDIO=1` when investigating
sound to also download the PCM recording. Normal input and timing runs leave
it on the SP until the next test replaces that temporary installation.

To test first-launch preparation from a game-free release:

```sh
scripts/package-release.sh
SKIP_PACKAGE=1 PACKAGE="$PWD/artifacts/release/balatro-miyoo-mini" \
  GAME_FILE=/path/to/Balatro.exe INSTALL_TEST=cold \
  CONTROLS_TEST=1 TEST_FRAMES=2000 WAIT_SECONDS=480 \
  scripts/test-sp.sh
```

Use `INSTALL_TEST=warm` to reuse the generated cache. These runs keep their
audio files in `/mnt/mmc/balatro-miyoo-install-test-audio-cache`, separate from
ordinary performance tests. `cold` clears only that test cache.

`CONTROLS_TEST=rapid-input` checks taps, menu locks and collection pages.
`INPUT_FIXTURE=rapid-input` feeds raw Miyoo button records through the Linux
reader and checks their effects on navigation, held repeat and release.
`CONTROLS_TEST=music-transitions` checks music restarts and slow-frame fades.
See the audio and controls notes for the frame limits used by each replay.

An SP replay is not a Mini performance result. Changes to input, audio, display
or timing also need a physical Mini test. Compare performance with the same
build options, game state, clock and capture settings; disable profilers for
timing runs.

## SD updates

```sh
scripts/deploy-miyoo.sh /path/to/sd-card
```

With no argument, the script uses `/Volumes/MIYOO`. It preserves the installed
game file, hashes saves before and after copying, and backs up saves and logs
under `/tmp/balatro-miyoo-deploy-*`. Eject the card through your OS afterward.
It does not rebuild the package.

`sh scripts/test-deploy.sh artifacts/release/balatro-miyoo-mini` checks updates
against temporary SD layouts, including save, audio-cache and game-file
preservation. It requires a built runtime-only package, but no real SD card.

Clock settings can be `off` or an explicit value from 1200 to 1600 MHz. Higher
clocks are not stable on every Mini. Cleanup cannot run after battery removal
or `SIGKILL`.
