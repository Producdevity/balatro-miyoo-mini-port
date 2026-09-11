use anyhow::{ensure, Result};
use lewton::inside_ogg::OggStreamReader;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub(super) const HEADER_BYTES: u64 = 64;
const BUFFER_FRAMES: usize = 4096;
const MAGIC: &[u8; 8] = b"BALPCM01";

pub(super) struct Asset {
    pub(super) path: PathBuf,
    channels: usize,
    rate: u32,
    frames: usize,
}

pub(super) fn source_key(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub(super) fn cache_path(root: &Path, key: &[u8; 32]) -> PathBuf {
    let name: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    root.join(format!("{name}.pcm"))
}

impl Asset {
    pub(super) fn find(root: &Path, data: &[u8]) -> Result<Option<Self>> {
        let key = source_key(data);
        Self::find_key(root, &key)
    }

    pub(super) fn byte_len(&self) -> u64 {
        self.frames as u64 * self.channels as u64 * 2
    }

    pub(super) fn find_key(root: &Path, key: &[u8; 32]) -> Result<Option<Self>> {
        let path = cache_path(root, key);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut header = [0; HEADER_BYTES as usize];
        file.read_exact(&mut header)?;
        ensure!(
            &header[..8] == MAGIC && header[24..56] == *key,
            "invalid PCM header"
        );
        let channels = u32::from_le_bytes(header[8..12].try_into()?) as usize;
        let rate = u32::from_le_bytes(header[12..16].try_into()?);
        let frames = u64::from_le_bytes(header[16..24].try_into()?);
        ensure!(
            (1..=2).contains(&channels) && (8000..=192000).contains(&rate),
            "unsupported PCM format"
        );
        ensure!(
            frames > 0 && frames <= usize::MAX as u64,
            "invalid PCM length"
        );
        let bytes = frames
            .checked_mul(channels as u64 * 2)
            .and_then(|bytes| bytes.checked_add(HEADER_BYTES));
        ensure!(bytes == Some(file.metadata()?.len()), "truncated PCM file");
        Ok(Some(Self {
            path,
            channels,
            rate,
            frames: frames as usize,
        }))
    }
}

pub(super) struct Playback {
    file: File,
    channels: usize,
    rate: u32,
    frames: usize,
    buffer: Vec<u8>,
    first: usize,
    buffered: usize,
    #[cfg(test)]
    reads: usize,
}

impl Playback {
    pub(super) fn new(asset: &Asset) -> Result<Self> {
        Ok(Self {
            file: File::open(&asset.path)?,
            channels: asset.channels,
            rate: asset.rate,
            frames: asset.frames,
            buffer: vec![0; (BUFFER_FRAMES + 1) * asset.channels * 2],
            first: 0,
            buffered: 0,
            #[cfg(test)]
            reads: 0,
        })
    }

    pub(super) fn frames(&self) -> usize {
        self.frames
    }
    pub(super) fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub(super) fn sample(&mut self, frame: usize) -> Result<Option<(i16, i16)>> {
        if frame >= self.frames {
            return Ok(None);
        }
        if frame < self.first || frame - self.first >= self.buffered {
            let first = frame / BUFFER_FRAMES * BUFFER_FRAMES;
            // The extra frame lets fractional-pitch interpolation cross a block
            // without repeatedly evicting and rereading the previous block.
            let buffered = (BUFFER_FRAMES + 1).min(self.frames - first);
            self.buffered = 0;
            self.file.seek(SeekFrom::Start(
                HEADER_BYTES + first as u64 * self.channels as u64 * 2,
            ))?;
            self.file
                .read_exact(&mut self.buffer[..buffered * self.channels * 2])?;
            self.first = first;
            self.buffered = buffered;
            #[cfg(test)]
            {
                self.reads += 1;
            }
        }
        let offset = (frame - self.first) * self.channels * 2;
        let left = i16::from_le_bytes(self.buffer[offset..offset + 2].try_into()?);
        let right = if self.channels == 2 {
            i16::from_le_bytes(self.buffer[offset + 2..offset + 4].try_into()?)
        } else {
            left
        };
        Ok(Some((left, right)))
    }
}

#[cfg(test)]
fn prepare_source(root: &Path, data: &[u8]) -> Result<Option<(PathBuf, u64)>> {
    prepare_source_with_progress(root, data, |_| {})
}

pub(super) fn prepare_source_with_progress(
    root: &Path,
    data: &[u8],
    mut progress: impl FnMut(u64),
) -> Result<Option<(PathBuf, u64)>> {
    if let Ok(Some(asset)) = Asset::find(root, data) {
        let bytes = asset.byte_len();
        return Ok(Some((asset.path, bytes)));
    }
    let key = source_key(data);
    let path = cache_path(root, &key);
    let temporary = path.with_extension("pcm.part");
    let mut reader = OggStreamReader::new(Cursor::new(data))?;
    let channels = reader.ident_hdr.audio_channels as usize;
    let rate = reader.ident_hdr.audio_sample_rate;
    ensure!(
        (1..=2).contains(&channels),
        "unsupported Vorbis channel count"
    );
    let result = (|| -> Result<Option<(PathBuf, u64)>> {
        let mut output = BufWriter::new(File::create(&temporary)?);
        output.write_all(&[0; HEADER_BYTES as usize])?;
        let mut samples = 0_u64;
        let mut bytes = Vec::new();
        let mut reported = std::time::Instant::now();
        while let Some(packet) = reader.read_dec_packet_itl()? {
            samples += packet.len() as u64;
            bytes.clear();
            for sample in packet {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            output.write_all(&bytes)?;
            if reported.elapsed() >= std::time::Duration::from_millis(200) {
                progress(samples * 2);
                reported = std::time::Instant::now();
            }
        }
        if samples * 2 <= super::STATIC_CLIP_BYTES as u64 {
            return Ok(None);
        }
        ensure!(samples % channels as u64 == 0, "incomplete PCM frame");
        let mut header = [0; HEADER_BYTES as usize];
        header[..8].copy_from_slice(MAGIC);
        header[8..12].copy_from_slice(&(channels as u32).to_le_bytes());
        header[12..16].copy_from_slice(&rate.to_le_bytes());
        header[16..24].copy_from_slice(&(samples / channels as u64).to_le_bytes());
        header[24..56].copy_from_slice(&key);
        output.seek(SeekFrom::Start(0))?;
        output.write_all(&header)?;
        output.flush()?;
        output.get_ref().sync_all()?;
        drop(output);
        std::fs::rename(&temporary, &path)?;
        Ok(Some((path, samples * 2)))
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "pcm_tests.rs"]
mod tests;
