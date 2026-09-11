use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "balatro-prepare-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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

fn archive(path: &Path, sounds: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, data) in sounds {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn rejects_missing_and_invalid_audio_without_publishing_an_index() {
    let directory = Directory::new();
    let game = directory.0.join("game.zip");
    let output = directory.0.join("audio-cache");
    archive(&game, &[("main.lua", b"return true")]);
    assert!(prepare_cache_with_progress(&game, &output, |_| {}).is_err());
    archive(&game, &[("resources/sounds/invalid.ogg", b"invalid")]);
    assert!(prepare_cache_with_progress(&game, &output, |_| {}).is_err());
    assert!(!output.join("index.txt").exists());
}

#[test]
fn index_rejects_changed_archives_truncation_and_oversized_input() {
    let directory = Directory::new();
    let index = directory.0.join("index.txt");
    std::fs::write(&index, "BALCACHE1 abc 1 0\n").unwrap();
    assert_eq!(
        read_index(&directory.0, "abc", 1),
        Some(CacheSummary {
            files: 0,
            bytes: 0,
            reused: true
        })
    );
    assert!(read_index(&directory.0, "changed", 1).is_none());
    assert!(read_index(&directory.0, "abc", 2).is_none());
    for content in ["", "BALCACHE1 abc 1 1\n", "BALCACHE1 abc 1 0\njunk\n"] {
        std::fs::write(&index, content).unwrap();
        assert!(read_index(&directory.0, "abc", 1).is_none());
    }
    std::fs::write(&index, vec![b'a'; MAX_INDEX_BYTES as usize + 1]).unwrap();
    assert!(read_index(&directory.0, "abc", 1).is_none());
}

#[test]
#[cfg(unix)]
fn preparation_lock_is_released_on_exit() {
    let directory = Directory::new();
    let first = lock_cache(&directory.0).unwrap();
    assert!(lock_cache(&directory.0).is_err());
    drop(first);
    assert!(lock_cache(&directory.0).is_ok());
}

#[test]
#[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
fn preparation_resumes_repairs_and_reuses_cached_audio() {
    let directory = Directory::new();
    let game = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
    let mut original = zip::ZipArchive::new(File::open(game).unwrap()).unwrap();
    let mut data = Vec::new();
    original
        .by_name("resources/sounds/introPad1.ogg")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    let game = directory.0.join("game.zip");
    let output = directory.0.join("audio-cache");
    archive(&game, &[("resources/sounds/introPad1.ogg", &data)]);
    let original_bytes = std::fs::read(&game).unwrap();
    let first = prepare_cache_with_progress(&game, &output, |_| {}).unwrap();
    assert_eq!(first.files, 1);
    assert!(!first.reused);
    let warm = prepare_cache_with_progress(&game, &output, |_| panic!("warm cache decoded again"))
        .unwrap();
    assert_eq!(
        warm,
        CacheSummary {
            reused: true,
            ..first
        }
    );
    let asset = Asset::find(&output, &data).unwrap().unwrap();
    let modified = std::fs::metadata(&asset.path).unwrap().modified().unwrap();
    std::fs::remove_file(output.join("index.txt")).unwrap();
    prepare_cache_with_progress(&game, &output, |_| {}).unwrap();
    assert_eq!(
        std::fs::metadata(&asset.path).unwrap().modified().unwrap(),
        modified
    );
    OpenOptions::new()
        .write(true)
        .open(&asset.path)
        .unwrap()
        .set_len(4)
        .unwrap();
    let repaired = prepare_cache_with_progress(&game, &output, |_| {}).unwrap();
    assert_eq!(repaired, first);
    assert!(Asset::find(&output, &data).unwrap().is_some());
    std::fs::remove_file(&asset.path).unwrap();
    std::fs::write(asset.path.with_extension("pcm.part"), b"interrupted").unwrap();
    assert_eq!(
        prepare_cache_with_progress(&game, &output, |_| {}).unwrap(),
        first
    );
    assert!(!asset.path.with_extension("pcm.part").exists());
    assert_eq!(std::fs::read(&game).unwrap(), original_bytes);
}
