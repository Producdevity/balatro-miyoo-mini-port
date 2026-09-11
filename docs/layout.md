# Small-screen layout

The game renders at 640x480. Most screen adjustments live in
`crates/love-api/src/miyoo/small_screen.lua`; popup placement is in
`popups.lua`, and deck views are in `deck_layout.lua`.

The compact HUD, hand and bottom actions share the same room coordinates.
Hand placement replaces Balatro's selection inset instead of adding a second
one. Played cards have a separate row with space for the scoring highlight.
Changing glyph scale rebuilds text metrics and retains the original width
limit, including undiscovered-card titles.

Tooltips try positions above, below or beside the focused card and stay within
the screen. A long description may cover other cards, but should not obscure
the card being described. The held deck overview draws in the foreground pass
above Jokers; card tooltips are suppressed until it closes.

Collection reveals retain their animation. Dissolve noise is cached by seed
and texture size while the threshold, edge fade and burn colours change every
frame. Events created in paused menus use elapsed time; gameplay events retain
their original clock and pause behavior.

## Checks

```sh
CONTROLS_TEST=layout TEST_FRAMES=3000 WAIT_SECONDS=240 scripts/test-sp.sh
CONTROLS_TEST=scoring AUTOPLAY_PAYOUT_JOKERS=2 \
  AUTOPLAY_STRESS_EFFECTS=1 TEST_FRAMES=1800 scripts/test-sp.sh
CONTROLS_TEST=blind TEST_FRAMES=1000 WAIT_SECONDS=120 scripts/test-sp.sh
```

The replays check main-menu bounds, shop titles, hand/scoring rows and the
Skip Blind tooltip's lifetime. The full controller replay also checks the
held deck overview, repeated tab changes and tooltip restoration.

Inspect the saved captures as well as the assertions. Test text, focus
visibility and physical controls on a Mini before distributing layout changes.
