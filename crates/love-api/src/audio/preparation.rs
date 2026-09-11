use super::*;

pub(super) fn run(receiver: mpsc::Receiver<AudioRequest>, sender: mpsc::Sender<MixerMessage>) {
    let mut cache = StaticCache::new();
    let pcm_root = std::env::var_os("BALATRO_PCM_CACHE").map(std::path::PathBuf::from);
    for request in receiver {
        let message = match request {
            AudioRequest::Control(command) => MixerMessage::Control(command),
            AudioRequest::Register(source) => {
                let pcm =
                    pcm_root
                        .as_ref()
                        .and_then(|root| match pcm::Asset::find(root, &source.data) {
                            Ok(asset) => asset,
                            Err(error) => {
                                eprintln!(
                                    "[audio] PCM cache {}: {error:#}; using Vorbis",
                                    source.path
                                );
                                None
                            }
                        });
                let asset = if let Some(asset) = pcm {
                    eprintln!(
                        "[audio] file-backed PCM: {} (source={})",
                        source.path, source.control.id
                    );
                    AudioAsset::Pcm(Arc::new(asset))
                } else if source.streaming {
                    AudioAsset::Stream(Arc::clone(&source.data))
                } else if let Some(clip) = cache.get(&source.path) {
                    AudioAsset::Static(clip)
                } else {
                    match decode_asset(Arc::clone(&source.data)) {
                        Ok(asset) => {
                            if let AudioAsset::Static(clip) = &asset {
                                cache.insert(source.path.clone(), clip.clone());
                            }
                            asset
                        }
                        Err(error) => {
                            eprintln!("[audio] decode {}: {error:#}", source.path);
                            source.control.playing.store(false, Ordering::Relaxed);
                            continue;
                        }
                    }
                };
                MixerMessage::Register(
                    source.control.id,
                    RegisteredSource {
                        asset,
                        control: Arc::downgrade(&source.control),
                    },
                )
            }
        };
        // Preserve register/play/stop order without decoding on the mixer.
        if sender.send(message).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn preparation_bounds_static_audio_and_preserves_command_order() {
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        for (file, streaming) in [("chips1.ogg", false), ("introPad1.ogg", true)] {
            let mut bytes = Vec::new();
            archive
                .by_name(&format!("resources/sounds/{file}"))
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            let data: Arc<[u8]> = bytes.into();
            let (send, receive) = mpsc::channel();
            let (prepared, output) = mpsc::channel();
            let control = Arc::new(SourceControl::new(1));
            send.send(AudioRequest::Register(SourceDescription {
                path: file.into(),
                streaming: false,
                data,
                control: Arc::clone(&control),
            }))
            .unwrap();
            send.send(AudioRequest::Control(AudioCommand::Play(1)))
                .unwrap();
            send.send(AudioRequest::Control(AudioCommand::Stop(1)))
                .unwrap();
            drop(send);
            let worker = std::thread::spawn(move || run(receive, prepared));
            let source = match output.recv_timeout(Duration::from_secs(5)).unwrap() {
                MixerMessage::Register(1, source) => source,
                _ => panic!("play reached the mixer before registration"),
            };
            assert_eq!(matches!(source.asset, AudioAsset::Stream(_)), streaming);
            if let AudioAsset::Static(clip) = source.asset {
                assert!(clip.samples.len() * 2 <= STATIC_CLIP_BYTES);
            }
            assert!(matches!(
                output.recv().unwrap(),
                MixerMessage::Control(AudioCommand::Play(1))
            ));
            assert!(matches!(
                output.recv().unwrap(),
                MixerMessage::Control(AudioCommand::Stop(1))
            ));
            worker.join().unwrap();
            assert!(
                output.recv().is_err(),
                "loader retained its own input channel"
            );
        }
    }
}
