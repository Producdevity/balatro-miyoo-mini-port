# Performance testing

The target is sustained 30 FPS on the original Miyoo Mini at 640x480. It has not
been established across a full run. Keep card details, readable text and effect
animation when comparing optimizations.

The RG35XX SP test runner limits the game to two 1.512 GHz cores and a 96 MiB
virtual address space, with no swap. That bounds allocations but does not
reproduce the Mini's physical RAM layout, Cortex-A7 CPU, memory bus, kernel or
display/audio drivers. SP frame rates are not Mini frame rates.

## Reproduce a comparison

See [device setup](../CONTRIBUTING.md#device-tests) before running the SP script.
Keep the binary, clock, seed, scene and audio settings fixed when comparing
runtime switches. Use an off/on/on/off order to check run-to-run variation.
Disable profilers for timing runs; retain the logs with the build and settings.

```sh
AUTOPLAY_STRESS_EFFECTS=1 AUTOPLAY_PAYOUT_JOKERS=2 \
  AUTOPLAY_TEST_BLIND_CHIPS=300 AUDIO_CAPTURE=1 \
  TEST_FRAMES=2400 WAIT_SECONDS=180 scripts/test-sp.sh
```

This replay changes the hand and adds Jokers to exercise effects. It is a
stress workload, not a normal playthrough. Compare selection, scoring and shop
timings separately. Average frame time describes throughput; p95 is the time
within which 95% of frames finished. Include peak memory and stalls as well.

For a renderer change, repeat with `VERIFY_RASTER=1`. It compares optimized
output with the reference paths, so its frame times are not performance results.
Also run the controller replay and inspect captures for clipping and overlaps.

## Long runs

```sh
CONTROLS_TEST=long-run AUTOPLAY_PAYOUT_JOKERS=5 AUDIO_CAPTURE=1 \
  TEST_FRAMES=18000 WAIT_SECONDS=1200 scripts/test-sp.sh
```

This cycles through blinds, payouts and shops with a one-chip blind target.
It checks for duplicate or removed objects in the update lists and Cash Out
controls left behind after a payout. Each new hand logs the Lua heap size and
live object counts. Compare later rounds after the first shop has loaded;
memory should settle rather than grow with every round. This is a lifetime
test, not an FPS benchmark or a substitute for a full run on the Mini.

## Card-effects benchmark

```sh
cargo run --release -p sprite-to-text --bin bench-card-effects -- 120
```

This draws synthetic cards at 640x480 and reports time and a pixel checksum
for each effect. It isolates raster work without loading Balatro. Use
`CARD_LAYERS=2` for layered cards, `CARD_PREPARED=1` for prepared effects,
or `CARD_SCALE=0.5` for the smaller 320x240 workload. An optional second
argument supplies a 71x95 RGBA card image.

Check improvements in real gameplay after running the benchmark.
Local captures, game-derived inputs and experiment logs are excluded from Git.
