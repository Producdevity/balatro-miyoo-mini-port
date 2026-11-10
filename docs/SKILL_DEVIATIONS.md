# Deviations from the miyoo-game-port skill

Notes for updating the skill after the Balatro fork (2026-11-11).

- **Started from an existing GPL-3.0 port** (Producdevity/balatro-miyoo-mini-port
  v0.1.3, commit 908c3e8) instead of triage and a new runtime. Balatro is LÖVE/Lua,
  which the skill's engine matrix does not cover; the existing Rust LÖVE runtime
  already ran the unmodified game archive.
- **No GitHub fork yet.** `gh repo fork` was blocked by the local permission check,
  so this is a plain clone. Create the fork on GitHub and repoint `origin`.
- **Toolchain differs from upstream CI.** Upstream builds LuaJIT in an ARMv7 Docker
  container; this PC has no Docker. Used instead (all under ~/Recompilation/tools):
  Rust 1.97.1 (rustup, pinned), Zig 0.16.0, cargo-zigbuild 0.23.4, and GCC 8.2.1
  (`prebuilt/`) only for `libgcc.a` and the shim below.
  LuaJIT 2.1 (luajit-src 210.5.12+a4f56a4) cross-built with Zig:

  ```sh
  make BUILDMODE=static TARGET_SYS=Linux HOST_CC="zig cc -target x86-linux-musl" \
    CC="zig cc -target arm-linux-musleabihf -mcpu=cortex_a7" \
    TARGET_AR="zig ar rcus" TARGET_STRIP=true
  ```

  The `luajit` executable fails to link (EHABI unwind symbols); only `libluajit.a`
  is needed. Clang emits generic `__floatdidf`-style helpers that GCC's libgcc does
  not satisfy in the final link order, so `native/clang-rt-shim.c` (compiled with
  GCC, forwarding to the `__aeabi_*` routines) is added to `libluajit-5.1.a`.
  `scripts/build.sh` still requires Docker; run its cargo/audio steps directly.
- **No device test by me.** Miyoo Wi-Fi is down, so installation went straight to
  the SD card in the PC and the user tests by hand. No SSH frame timing, memory or
  screenshot evidence exists for this build yet.
- **Stock clock by default.** Upstream raised the CPU to 1.5 GHz. The launcher now
  leaves the clock alone unless `BALATRO_CPU_MHZ` is set.

## Fidelity changes

- Held D-pad repeat uses wall-clock time. The game caps its smoothed dt at 0.1 s,
  so at 5 fps a held direction repeated 5 times in 2 s versus about 15 on desktop.
  Now 9+ (one repeat per frame at most). Test:
  `held_direction_repeats_on_wall_clock_at_low_frame_rates`.
- Font pauses: NotoSans-Bold (Latin non-English languages, ~570 KiB) stays
  resident instead of reloading after 250 ms idle. CJK fonts now expire after 2 s
  instead of 250 ms; peak memory is unchanged since only one large font is held.
  English uses m6x11plus, which never had this pause.
- Layout not changed. Restoring the desktop HUD arrangement at 640x480 needs a
  device look first; the upstream single-room layout is kept.
