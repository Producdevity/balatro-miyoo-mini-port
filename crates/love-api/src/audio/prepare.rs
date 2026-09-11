use super::pcm::{self, Asset};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INDEX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct CacheProgress {
    pub completed: usize,
    pub total: usize,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheSummary {
    pub files: usize,
    pub bytes: u64,
    pub reused: bool,
}

struct Sound {
    index: usize,
    name: String,
}

fn sounds(archive: &mut zip::ZipArchive<File>) -> Result<(Vec<Sound>, String)> {
    let mut sounds = Vec::new();
    let mut digest = Sha256::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir()
            || !entry.name().starts_with("resources/sounds/")
            || !entry.name().ends_with(".ogg")
        {
            continue;
        }
        ensure!(
            entry.size() <= MAX_SOURCE_BYTES,
            "sound is too large: {}",
            entry.name()
        );
        digest.update(entry.name().as_bytes());
        digest.update([0]);
        digest.update(entry.size().to_le_bytes());
        digest.update(entry.crc32().to_le_bytes());
        sounds.push(Sound {
            index,
            name: entry.name().to_owned(),
        });
    }
    ensure!(!sounds.is_empty(), "game archive has no audio files");
    Ok((sounds, format!("{:x}", digest.finalize())))
}

fn read_index(output: &Path, signature: &str, sounds: usize) -> Option<CacheSummary> {
    let mut text = String::new();
    File::open(output.join("index.txt"))
        .ok()?
        .take(MAX_INDEX_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_INDEX_BYTES {
        return None;
    }
    let mut lines = text.lines();
    let fields: Vec<_> = lines.next()?.split_whitespace().collect();
    if fields.len() != 4
        || fields[0] != "BALCACHE1"
        || fields[1] != signature
        || fields[2].parse::<usize>().ok()? != sounds
    {
        return None;
    }
    let expected = fields[3].parse::<usize>().ok()?;
    if expected > sounds {
        return None;
    }
    let mut files = 0;
    let mut bytes = 0;
    for line in lines {
        if line.len() != 64 || !line.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let mut key = [0; 32];
        for (byte, hex) in key.iter_mut().zip(line.as_bytes().chunks_exact(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
        }
        let asset = Asset::find_key(output, &key).ok()??;
        bytes += asset.byte_len();
        files += 1;
    }
    (files == expected).then_some(CacheSummary {
        files,
        bytes,
        reused: true,
    })
}

struct CacheLock(File);

impl Drop for CacheLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

fn lock_cache(output: &Path) -> Result<CacheLock> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(output.join(".prepare.lock"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        ensure!(result == 0, "audio preparation is already running");
    }
    Ok(CacheLock(file))
}

/// Prepare audio once, then validate the small cache index on later launches.
pub fn prepare_cache_with_progress(
    game: &Path,
    output: &Path,
    mut progress: impl FnMut(CacheProgress),
) -> Result<CacheSummary> {
    let mut archive = zip::ZipArchive::new(File::open(game)?)?;
    let (sounds, signature) = sounds(&mut archive)?;
    if let Some(summary) = read_index(output, &signature, sounds.len()) {
        return Ok(summary);
    }
    std::fs::create_dir_all(output)?;
    let _lock = lock_cache(output)?;
    let mut keys = Vec::new();
    let mut bytes = 0;
    for (completed, sound) in sounds.iter().enumerate() {
        let state = CacheProgress {
            completed,
            total: sounds.len(),
            bytes,
        };
        progress(state);
        let mut data = Vec::new();
        archive
            .by_index(sound.index)?
            .take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut data)?;
        ensure!(
            data.len() as u64 <= MAX_SOURCE_BYTES,
            "sound is too large: {}",
            sound.name
        );
        if let Some((_, size)) = pcm::prepare_source_with_progress(output, &data, |written| {
            progress(CacheProgress {
                bytes: bytes + written,
                ..state
            });
        })
        .with_context(|| format!("prepare {}", sound.name))?
        {
            keys.push(format!("{:x}", Sha256::digest(&data)));
            bytes += size;
        }
    }
    let mut index = format!("BALCACHE1 {signature} {} {}\n", sounds.len(), keys.len());
    for key in &keys {
        index.push_str(key);
        index.push('\n');
    }
    let temporary = output.join("index.txt.part");
    let mut file = File::create(&temporary)?;
    file.write_all(index.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, output.join("index.txt"))?;
    progress(CacheProgress {
        completed: sounds.len(),
        total: sounds.len(),
        bytes,
    });
    Ok(CacheSummary {
        files: keys.len(),
        bytes,
        reused: false,
    })
}

pub fn prepare_cache(game: &Path, output: &Path) -> Result<()> {
    let mut previous = None;
    let summary = prepare_cache_with_progress(game, output, |state| {
        if previous != Some(state.completed) {
            println!(
                "Audio: {}/{} sounds, {} MiB",
                state.completed,
                state.total,
                state.bytes / (1024 * 1024)
            );
            previous = Some(state.completed);
        }
    })?;
    println!(
        "Audio ready: {} long sounds, {} MiB",
        summary.files,
        summary.bytes / (1024 * 1024)
    );
    Ok(())
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
