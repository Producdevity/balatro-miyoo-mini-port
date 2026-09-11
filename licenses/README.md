# Third-party notices

The port as a whole is GPL-3.0-only; see `../LICENSE`. These files retain the notices for
code used by the port:

- `balatro-port-tui.txt`: the original Apache-2.0 license and copyright notice.
- `LuaJIT.txt`: LuaJIT, including the OpenResty changes used by `luajit-src`.
- `PortMaster.txt`: the small-screen layout derived from PortMaster.
- `SLEEF.txt`: SLEEF 3.9.0, used by the vector flame renderer.
- `Nunito.txt`: the Nunito Black font used for handheld text.
- `Rust.txt`: dependencies in `Cargo.lock`, collected by
  `scripts/collect-licenses.py` from their published crate archives.
- `Rust-standard-library.html`: the Rust 1.97.1 standard library notices,
  copied from the toolchain's `share/doc/rust/COPYRIGHT-library.html`.
- `musl.txt`: musl's complete copyright notice from the Zig 0.16.0 toolchain.
- `GPL-3.0-or-later.txt` and `GCC-exception-3.1.txt`: the license and runtime
  exception for the static libgcc linked by `scripts/build.sh`.

When changing a toolchain, update its notices as well as the build scripts.
The Onion audio wrapper is loaded from the device and is not redistributed.
Game files, game artwork and audio are not covered by the project license.

Release source archives include the port, locked Cargo dependencies and SLEEF
source. Build instructions are in CONTRIBUTING.md. Compiler and operating-system
libraries retain their own licenses and runtime exceptions.
