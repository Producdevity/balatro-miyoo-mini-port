# Controls

The runtime reads Linux evdev events and maps the Mini's buttons to Balatro's
gamepad callbacks. It does not emulate a mouse for D-pad navigation.
`runtime/src/platform/input.rs` handles device events; `miyoo_input.rs` and
`miyoo_input.lua` adapt them to the game.

Presses and releases stay in order. Several D-pad taps received during one
slow frame can all move focus in that update. Other actions remain limited
to one press per update so selection, play, discard and menus cannot overwrite
each other's state.

Holding a direction uses the game's 0.3-second initial delay, then 0.1-second
repeats. Separate taps do not use that delay. A release followed by another
press starts a new hold timer, even if both arrive between updates.

The adapter waits through the short lock used when opening and closing menus.
Presses delayed by that lock expire after half a second. Slow rendering alone
does not expire input, and gameplay locks still block actions during scoring.
Controller taps bypass the option-arrow click debounce; mouse clicks and
disabled-button checks keep their original behavior.

## Tests

Host queue tests cover taps, holds, aliases, locks and event order. With
`BALATRO_TEST_GAME`, tests also use the owned game's controller and UI code.

```sh
CONTROLS_TEST=rapid-input AUDIO_CAPTURE=1 scripts/test-sp.sh
INPUT_FIXTURE=rapid-input TEST_FRAMES=900 WAIT_SECONDS=120 scripts/test-sp.sh
CONTROLS_TEST=1 TEST_FRAMES=2000 WAIT_SECONDS=180 scripts/test-sp.sh
```

The first replay exercises the input adapter directly. The second feeds
Miyoo-format evdev records through a nonblocking FIFO and checks double taps,
held repeat and release. The full replay covers selection, discard, play,
menus, deck tabs and tooltip persistence.

These tests do not measure physical button bounce or button-to-screen latency.
Check those on a Mini after changing the adapter or frame scheduling.
