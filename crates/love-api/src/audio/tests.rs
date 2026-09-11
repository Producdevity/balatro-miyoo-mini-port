use super::*;

fn control(id: u64, volume: f32, pitch: f32) -> Arc<SourceControl> {
    let control = Arc::new(SourceControl::new(id));
    control.volume.store(volume.to_bits(), Ordering::Relaxed);
    control.pitch.store(pitch.to_bits(), Ordering::Relaxed);
    control.playing.store(true, Ordering::Relaxed);
    control
}

#[test]
fn mixer_duplicates_mono_and_applies_volume() {
    let clip = AudioClip {
        samples: vec![1000, -1000].into(),
        channels: 1,
        sample_rate: OUTPUT_RATE,
    };
    let mut voices = vec![Voice {
        id: 1,
        control: control(1, 0.5, 1.0),
        playback: Playback::Static(clip),
        position: 0.0,
    }];
    let mut output = [0i16; 4];
    let mut accumulator = [0i32; 4];
    mix(&mut voices, 1.0, &mut output, &mut accumulator);
    assert_eq!(output, [500, 500, -500, -500]);
}

#[test]
fn mixer_respects_pitch_and_stops_at_end() {
    let clip = AudioClip {
        samples: vec![10, 20, 30, 40].into(),
        channels: 1,
        sample_rate: OUTPUT_RATE,
    };
    let source = control(1, 1.0, 2.0);
    let mut voices = vec![Voice {
        id: 1,
        control: Arc::clone(&source),
        playback: Playback::Static(clip),
        position: 0.0,
    }];
    let mut output = [0i16; 6];
    let mut accumulator = [0i32; 6];
    mix(&mut voices, 1.0, &mut output, &mut accumulator);
    assert_eq!(output, [10, 10, 30, 30, 0, 0]);
    assert!(voices.is_empty());
    assert!(!source.playing.load(Ordering::Relaxed));
}

#[test]
fn play_keeps_an_existing_voices_position() {
    let source = control(1, 1.0, 1.0);
    let mut registered = HashMap::from([(
        1,
        RegisteredSource {
            asset: AudioAsset::Static(AudioClip {
                samples: vec![10, 20, 30, 40].into(),
                channels: 1,
                sample_rate: OUTPUT_RATE,
            }),
            control: Arc::downgrade(&source),
        },
    )]);
    let mut voices = vec![Voice::new(1, &registered[&1]).unwrap()];
    voices[0].position = 2.0;
    handle_command(AudioCommand::Play(1), &mut registered, &mut voices);
    assert_eq!(voices.len(), 1);
    assert_eq!(voices[0].position, 2.0, "play rewound an existing source");
    handle_command(AudioCommand::Stop(1), &mut registered, &mut voices);
    handle_command(AudioCommand::Play(1), &mut registered, &mut voices);
    assert_eq!(voices[0].position, 0.0, "a stopped source did not restart");
}

#[test]
fn grouped_play_starts_prepared_sources_in_the_same_mix_block() {
    let controls: Vec<_> = (1..=5).map(|id| control(id, 1.0, 1.0)).collect();
    let mut registered = HashMap::new();
    let mut voices = Vec::new();
    let mut output = [0; 2];
    for source in &controls {
        handle_message(
            MixerMessage::Register(
                source.id,
                RegisteredSource {
                    asset: AudioAsset::Static(AudioClip {
                        samples: vec![10, 20, 30, 40].into(),
                        channels: 1,
                        sample_rate: OUTPUT_RATE,
                    }),
                    control: Arc::downgrade(source),
                },
            ),
            &mut registered,
            &mut voices,
        );
        mix(&mut voices, 1.0, &mut output, &mut [0; 2]);
        assert_eq!(
            output,
            [0, 0],
            "part of the group started during preparation"
        );
    }
    handle_command(
        AudioCommand::PlayMany(vec![1, 2, 3, 4, 5]),
        &mut registered,
        &mut voices,
    );
    mix(&mut voices, 1.0, &mut output, &mut [0; 2]);
    assert_eq!(output, [50, 50]);
    assert!(voices.iter().all(|voice| voice.position == 1.0));
    handle_command(
        AudioCommand::PlayMany(vec![1, 1, 2, 3, 4, 5]),
        &mut registered,
        &mut voices,
    );
    assert_eq!(voices.len(), 5, "repeated play duplicated voices");
    assert!(
        voices.iter().all(|voice| voice.position == 1.0),
        "repeated play rewound voices"
    );
}

#[test]
fn play_accepts_source_lists_and_varargs_without_silently_skipping_invalid_sources() {
    let lua = Lua::new();
    lua.load("a={_svmm_source_id=1}; b={_svmm_source_id=2}")
        .exec()
        .unwrap();
    for expression in ["return a,b", "return {a,b}"] {
        assert_eq!(
            playback_ids(lua.load(expression).eval().unwrap()).unwrap(),
            vec![1, 2]
        );
    }
    for expression in ["return a,42", "return {a,{}}", "return nil"] {
        assert!(playback_ids(lua.load(expression).eval().unwrap()).is_err());
    }
}

#[test]
fn fractional_pitch_interpolates_between_samples() {
    let mut voices = vec![Voice {
        id: 1,
        control: control(1, 1.0, 0.5),
        playback: Playback::Static(AudioClip {
            samples: vec![0, 1000, 2000].into(),
            channels: 1,
            sample_rate: OUTPUT_RATE,
        }),
        position: 0.0,
    }];
    let mut output = [0i16; 10];
    mix(&mut voices, 1.0, &mut output, &mut [0i32; 10]);
    assert_eq!(output, [0, 0, 500, 500, 1000, 1000, 1500, 1500, 2000, 2000]);
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn streamed_resampling_matches_static_across_buffer_trims() {
    use std::io::Read;
    let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut data = Vec::new();
    archive
        .by_name("resources/sounds/chips1.ogg")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    let data: Arc<[u8]> = data.into();
    let clip = decode_static(Arc::clone(&data)).unwrap();
    assert!(clip.samples.len() / clip.channels > 8192);
    let mut decoded = vec![Voice {
        id: 1,
        control: control(1, 0.8, 0.73),
        playback: Playback::Static(clip),
        position: 0.0,
    }];
    let mut streamed = vec![Voice {
        id: 2,
        control: control(2, 0.8, 0.73),
        playback: Playback::Stream(Box::new(StreamPlayback::new(data).unwrap())),
        position: 0.0,
    }];
    for block in 0..200 {
        let mut expected = [0i16; MIX_FRAMES * 2];
        let mut actual = expected;
        mix(&mut decoded, 1.0, &mut expected, &mut [0; MIX_FRAMES * 2]);
        mix(&mut streamed, 1.0, &mut actual, &mut [0; MIX_FRAMES * 2]);
        for (left, right) in expected.iter().zip(actual.iter()) {
            assert!(
                (*left as i32 - *right as i32).abs() <= 1,
                "stream differs from full decode in block {block}: {left} != {right}"
            );
        }
        assert_eq!(decoded.len(), streamed.len());
    }
    assert!(streamed.is_empty(), "stream did not stop at EOF");
}

#[test]
fn static_cache_stays_within_its_memory_limit() {
    let mut cache = StaticCache::new();
    let samples_per_clip = STATIC_CACHE_BYTES / 8;
    for index in 0..6 {
        cache.insert(
            format!("sound-{index}"),
            AudioClip {
                samples: vec![0; samples_per_clip].into(),
                channels: 2,
                sample_rate: OUTPUT_RATE,
            },
        );
    }
    assert!(cache.bytes <= STATIC_CACHE_BYTES);
    assert_eq!(cache.clips.len(), 4);
}
