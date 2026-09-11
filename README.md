# Balatro for Miyoo Mini

Balatro running on the original Miyoo Mini with OnionOS. The port uses a Rust
software renderer, a LOVE-compatible API and LuaJIT in interpreter mode. It
renders at 640x480 without OpenGL.

This is still in development. Sustained 30 FPS and long-session stability have
not been established on the Mini. Cards, text, controls and audio work, but
there are remaining performance and compatibility issues.

## Install

Extract the release ZIP onto an OnionOS SD card. Copy your own `Balatro.exe`
or `Balatro.love` into:

```text
Roms/PORTS/Games/Balatro/
```

Keep its filename. In Steam, use Manage > Browse local files to find it. On
macOS, look inside Balatro.app > Contents > Resources for `Balatro.love`.
Refresh the Ports list and launch Balatro from Strategy.

Balatro remains in the list if the game file is missing. Launching it shows
where to put the file.

The first launch prepares audio on the handheld. Leave at least 250 MB free
after copying the game and let it finish. Later launches reuse the cache;
interrupted preparation resumes on the next launch. No patching tools are
needed on your computer. The original game file is not changed.

An existing PortMaster file named `Balatro` also works. Game files, artwork and
saves are not included in the release. Back up your saves before updating.

The launcher requests 1.5 GHz through Onion's clock helper and restores the
previous clock on exit. Set `BALATRO_CPU_MHZ=off` to disable this. The port does
not enable swap or zram. Saves live in `Saves/CurrentProfile/saves/Balatro`.

## Build

See [CONTRIBUTING.md](CONTRIBUTING.md) for the required tools, tests and device
setup. Once the tools are installed:

```sh
scripts/package.sh /path/to/Balatro.exe
scripts/deploy-miyoo.sh /path/to/sd-card
```

The package is written to `artifacts/balatro-miyoo`. Deployment preserves existing
game files and saves. It installs the last package; it does not rebuild it.

`scripts/package-release.sh` creates `artifacts/release/balatro-miyoo-mini.zip`
without game data. Local packages can also prepare the audio cache in advance.
Generated audio is game data and must not be included in public downloads.

## About

Based on [balatro-port-tui](https://github.com/4RH1T3CT0R7/balatro-port-tui), with
small-screen layout work from [PortMaster](https://github.com/PortsMaster/PortMaster-New).
See [NOTICE](NOTICE) and [LICENSE](LICENSE) for attribution and licensing.
This port is GPLv3-licensed. Its Apache-2.0 base and other dependencies retain
their original notices in [licenses](licenses/README.md).
Balatro is made by LocalThunk and must be purchased separately.

[Architecture](docs/architecture.md) | [Audio](docs/audio.md) |
[Controls](docs/controls.md) | [Performance](docs/performance.md)
