# Audio

The mixer produces signed 16-bit stereo PCM at 44.1 kHz independently of game
frames. Ogg Vorbis decoding uses Lewton. Pitch and resampling follow the game's
requests; Balatro normally plays its music at pitch 0.7.

## Preparation and memory

First launch prepares long sounds as lossless PCM in `audio-cache`. The tested
game uses about 182 MiB on disk for these files. Later launches check an index,
PCM headers and file lengths instead of decoding the music again. This detects
missing or truncated files, not arbitrary same-size corruption.

Preparation takes an exclusive lock, writes each track to a temporary file,
flushes it, then renames it. Interrupted runs reuse completed tracks. The index
is written after every sound passes. A progress screen and launcher log report
failures. The original archive is not modified.

Each streamed voice has a buffer of at most 4,097 frames (16,388 bytes for
stereo). Silent voices advance without disk reads. Short effects use an 8 MiB
reuse cache; active voices can retain clips beyond it, so that is not a total
audio-memory limit. Missing PCM entries fall back to shared compressed Vorbis
data. New assets are prepared on a worker, never on the mixer thread.

Generated PCM is game data and must not be distributed. To prepare a local
cache on the host:

```sh
cargo run --release -p love-api --example prepare-audio -- \
  /path/to/Balatro.exe artifacts/audio-cache
```

## Onion output

The static musl runtime sends PCM to `balatro-audio`, a small glibc helper.
Only the helper loads Onion's installed `libpadsp.so`. It leaves the audio
server running and rejects a raw OSS device descriptor. The package does not
include firmware libraries.

Both transport pipes are bounded to 4 KiB. The parent uses nonblocking writes
with a 200 ms deadline. A stalled or lost helper is restarted with a
0.5-to-5-second backoff. During disconnection the mixer advances playback and
discards output, so recovery cannot replay a backlog. Shutdown waits at most
100 ms for the helper before killing it.

Only the mixer requests realtime priority 1. If the request is denied, it logs
the failure and uses normal scheduling. Game and rendering threads keep normal
priority. Output cadence logs count accepted samples; they cannot measure the
physical DAC clock or button-to-speaker latency.

## Music transitions

Balatro crossfades between five running arrangements. The port prepares all
layers before starting them in one mixer command, keeping them synchronized.
Repeated play requests preserve an already-playing source's position.

Crossfade volume uses elapsed time with the original exponential fade rate.
This avoids stretching transitions when game frames are slow. Track selection,
pitch and sound effects still use the original game logic.

## Testing

`scripts/check.sh` covers PCM/Vorbis agreement, resampling, loops, muted
intervals, partial writes, recovery, grouped playback and shutdown. Supplying
`BALATRO_TEST_GAME` also enables tests against the owned game's music functions.

```sh
scripts/test-audio-sp.sh
CONTROLS_TEST=music-transitions AUTOPLAY_DELAY=10000 \
  TEST_FRAMES=155 AUDIO_CAPTURE=1 scripts/test-sp.sh
```

The helper test injects short writes, EINTR, EAGAIN and output failure through
a fake OSS endpoint. The music replay deliberately slows frames to test fades.
Neither opens the SP speaker.

`AUDIO_CAPTURE=1` runs the normal mixer into paced PCM capture. Add
`FETCH_AUDIO=1` to retrieve the recording. Capture checks sample continuity and
mixer work, but does not exercise the Mini's audio server or physical output.
Sleep/wake recovery, tempo and latency need a Mini listening test.
