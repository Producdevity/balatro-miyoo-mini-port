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

The synthetic card benchmark is `crates/renderer/examples/card-effects.rs`.
Use it to isolate raster work, then check the change in real gameplay.
Local captures, game-derived inputs and experiment logs are excluded from Git.
