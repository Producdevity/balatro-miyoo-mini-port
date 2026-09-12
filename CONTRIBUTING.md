# Development

Host tests need Rust, Cargo and a C compiler. Set `BALATRO_TEST_GAME` to include
the regression tests that read the original game code.

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
Packaging also uses zip, unzip and a SHA-256 utility. CI builds the ARM package
on Linux and runs host tests on Linux and macOS.

```sh
rustup target add armv7-unknown-linux-musleabihf
cargo install --locked cargo-zigbuild --version 0.23.4
scripts/build.sh
scripts/package.sh /path/to/Balatro.exe
```

Docker builds the static LuaJIT library using an ARMv7 musl toolchain. It needs
to run `linux/arm/v7` containers, including on non-ARM hosts. Zig builds the Rust
runtime, NEON code and small Onion audio helper.
The runtime links statically; the audio helper uses Onion's glibc audio wrapper.

Dependencies are locked by `Cargo.lock`; the SLEEF download is checksum-checked.
Build output, downloaded sources and audio caches stay under `target/` and
`artifacts/`. Do not commit game archives, extracted game code, saves, PCM files
or copied artwork.

After changing Rust dependencies, run `python3 scripts/collect-licenses.py`.
It refreshes the license texts included in binary packages from Cargo's locked
ARM dependency graph. Keep upstream copyright notices when moving or editing
derived code.

## Packages

`scripts/package.sh /path/to/Balatro.exe` builds a local package in
`artifacts/balatro-miyoo`, including the game and prepared audio. You can also
set `BALATRO_GAME` to supply the game path.

`scripts/package-release.sh` creates `artifacts/release/balatro-miyoo-mini.zip`
with the runtime, launcher and licenses only.

Every PR and push to `master` runs the host checks and builds a game-free ARM
package. Download `balatro-miyoo-mini` from the workflow's artifacts to test it.

`scripts/package-source.sh` archives the committed tree, locked Cargo sources
and SLEEF for distribution beside the binary. Cargo reads the archive's bundled
`vendor` directory; use the same build commands and toolchain.

The shortcut ships as `Balatro.notfound`. Onion's Ports import replaces any
old active shortcut and renames the new one to `.port`. It checks for
`script.sh`; the runtime then finds and validates the user's game archive.
If the game is missing, the launcher stays visible and shows where to copy it.

See [architecture](docs/architecture.md) for the source layout and game integration.

## Device tests

`scripts/test-sp.sh` uses an RG35XX SP running muOS. Set `SP_HOST` to its SSH
hostname; it defaults to `muos-sp`. The runner temporarily takes ownership of
the frontend, so do not run it while another game is active.

```sh
export BALATRO_GAME=/path/to/Balatro.exe
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

`CONTROLS_TEST=languages TEST_FRAMES=5000 WAIT_SECONDS=300` opens the language
picker and switches through every language. `CONTROLS_TEST=jokers TEST_FRAMES=1400`
renders every joker collection page. Both use disposable saves and capture frames.
Use `CONTROLS_TEST=languages-play TEST_FRAMES=6500 WAIT_SECONDS=300` to also
play a round and open the shop in Chinese after switching languages.
`CONTROLS_TEST=blind-ui TEST_FRAMES=1000 WAIT_SECONDS=120` checks blind-panel
layering with four Jokers, the pause menu, and score contrast against The Flint.
See [audio](docs/audio.md) and [controls](docs/architecture.md#regression-checks)
for the frame limits used by each replay.

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
