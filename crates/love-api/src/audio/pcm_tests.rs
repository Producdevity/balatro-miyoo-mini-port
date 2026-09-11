use super::*;
use crate::audio::{mix, AudioClip, Playback as VoicePlayback, SourceControl, Voice, MIX_FRAMES};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "balatro-pcm-test-{}-{nonce}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(root: &Path, channels: usize, frames: usize) -> (Asset, Vec<i16>) {
    let key = source_key(b"test source");
    let path = cache_path(root, &key);
    let mut header = [0; HEADER_BYTES as usize];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&(channels as u32).to_le_bytes());
    header[12..16].copy_from_slice(&44100_u32.to_le_bytes());
    header[16..24].copy_from_slice(&(frames as u64).to_le_bytes());
    header[24..56].copy_from_slice(&key);
    let samples: Vec<i16> = (0..frames * channels)
        .map(|n| (n.wrapping_mul(32771) >> 2) as i16)
        .collect();
    let mut file = File::create(path).unwrap();
    file.write_all(&header).unwrap();
    for sample in &samples {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
    drop(file);
    (Asset::find(root, b"test source").unwrap().unwrap(), samples)
}

#[test]
fn bounded_reads_match_random_access_and_loop_boundaries() {
    let directory = Directory::new();
    for channels in [1, 2] {
        let (asset, samples) = fixture(&directory.0, channels, 10003);
        let mut playback = Playback::new(&asset).unwrap();
        for frame in (0..10003)
            .chain((0..10003).rev())
            .chain([4095, 4096, 4095, 4097, 0, 10002])
        {
            let left = samples[frame * channels];
            let right = samples[frame * channels + channels - 1];
            assert_eq!(playback.sample(frame).unwrap(), Some((left, right)));
        }
        assert_eq!(playback.sample(10003).unwrap(), None);
        assert_eq!(playback.sample(usize::MAX).unwrap(), None);
        assert!(playback.buffer.len() <= 16388);
    }
}

#[test]
fn failed_refills_do_not_publish_partial_samples() {
    let directory = Directory::new();
    let (asset, _) = fixture(&directory.0, 2, BUFFER_FRAMES * 3);
    let mut playback = Playback::new(&asset).unwrap();
    assert!(playback.sample(0).unwrap().is_some());
    File::options()
        .write(true)
        .open(&asset.path)
        .unwrap()
        .set_len(HEADER_BYTES + (BUFFER_FRAMES * 4 + 2) as u64)
        .unwrap();
    for _ in 0..2 {
        assert!(playback.sample(BUFFER_FRAMES + 1).is_err());
        assert_eq!(playback.buffered, 0);
    }
}

#[test]
fn muted_position_pitch_and_loops_match_decoded_audio_without_reads() {
    let directory = Directory::new();
    let (asset, samples) = fixture(&directory.0, 2, 8193);
    for pitch in [0.1_f32, 0.73, 1.0, 1.5, 4.0] {
        for looping in [false, true] {
            let make_voice = |playback| {
                let control = Arc::new(SourceControl::new(1));
                control.playing.store(true, Ordering::Relaxed);
                control.looping.store(looping, Ordering::Relaxed);
                control.pitch.store(pitch.to_bits(), Ordering::Relaxed);
                Voice {
                    id: 1,
                    control,
                    playback,
                    position: 0.0,
                }
            };
            let mut reference = vec![make_voice(VoicePlayback::Static(AudioClip {
                samples: samples.clone().into(),
                channels: 2,
                sample_rate: 44100,
            }))];
            let mut actual = vec![make_voice(VoicePlayback::Pcm(Box::new(
                Playback::new(&asset).unwrap(),
            )))];
            for block in 0..400 {
                let volume = if block < 60 || (140..210).contains(&block) {
                    0.0
                } else {
                    0.73
                };
                let mut expected = [0; MIX_FRAMES * 2];
                let mut output = [0; MIX_FRAMES * 2];
                mix(
                    &mut reference,
                    volume,
                    &mut expected,
                    &mut [0; MIX_FRAMES * 2],
                );
                mix(&mut actual, volume, &mut output, &mut [0; MIX_FRAMES * 2]);
                assert_eq!(
                    output, expected,
                    "pitch={pitch} looping={looping} block={block}"
                );
                assert_eq!(actual.len(), reference.len());
                if let (Some(actual), Some(reference)) = (actual.first(), reference.first()) {
                    assert_eq!(actual.position, reference.position);
                    if block < 60 {
                        let VoicePlayback::Pcm(pcm) = &actual.playback else {
                            unreachable!()
                        };
                        assert_eq!(pcm.reads, 0, "muted PCM performed disk reads");
                    }
                }
            }
        }
    }
}

#[test]
fn truncated_and_mismatched_files_are_not_used() {
    let directory = Directory::new();
    let (asset, _) = fixture(&directory.0, 2, 100);
    assert!(Asset::find(&directory.0, b"different source")
        .unwrap()
        .is_none());
    File::options()
        .write(true)
        .open(&asset.path)
        .unwrap()
        .set_len(HEADER_BYTES + 1)
        .unwrap();
    assert!(Asset::find(&directory.0, b"test source").is_err());
    let (asset, _) = fixture(&directory.0, 2, 100);
    let mut file = File::options().write(true).open(&asset.path).unwrap();
    file.seek(SeekFrom::Start(24)).unwrap();
    file.write_all(&[0; 32]).unwrap();
    assert!(Asset::find(&directory.0, b"test source").is_err());
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn prepared_music_matches_every_vorbis_sample() {
    let directory = Directory::new();
    let game = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(File::open(game).unwrap()).unwrap();
    let mut data = Vec::new();
    archive
        .by_name("resources/sounds/introPad1.ogg")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    let prepared = prepare_source(&directory.0, &data).unwrap().unwrap();
    let original = crate::audio::decode_static(data.clone().into()).unwrap();
    let asset = Asset::find(&directory.0, &data).unwrap().unwrap();
    assert_eq!(asset.frames * asset.channels, original.samples.len());
    assert_eq!(asset.rate, original.sample_rate);
    let mut playback = Playback::new(&asset).unwrap();
    for (frame, expected) in original.samples.chunks_exact(asset.channels).enumerate() {
        assert_eq!(
            playback.sample(frame).unwrap(),
            Some((expected[0], expected[asset.channels - 1]))
        );
    }
    assert_eq!(
        prepare_source(&directory.0, &data).unwrap().unwrap(),
        prepared
    );
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}
