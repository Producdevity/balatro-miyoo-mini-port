use anyhow::{Context, Result};
use lewton::inside_ogg::OggStreamReader;
use std::fs;
use std::io::BufReader;
use std::path::PathBuf;

fn main() -> Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: check-audio <sound directory>")?;
    let mut paths = fs::read_dir(&directory)
        .with_context(|| format!("read {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("ogg"))
        .collect::<Vec<_>>();
    paths.sort();

    let mut decoded = 0;
    for path in paths {
        let started = std::time::Instant::now();
        let file = fs::File::open(&path)?;
        let mut reader = OggStreamReader::new(BufReader::new(file))
            .with_context(|| format!("open {}", path.display()))?;
        let mut samples = 0usize;
        while let Some(packet) = reader
            .read_dec_packet_itl()
            .with_context(|| format!("decode {}", path.display()))?
        {
            samples += packet.len();
        }
        anyhow::ensure!(samples > 0, "{} decoded no samples", path.display());
        println!(
            "{}: {} Hz, {} channel(s), {} samples, decode_ms={:.2}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            reader.ident_hdr.audio_sample_rate,
            reader.ident_hdr.audio_channels,
            samples,
            started.elapsed().as_secs_f64() * 1000.0
        );
        decoded += 1;
    }
    anyhow::ensure!(decoded > 0, "no Ogg files found in {}", directory.display());
    println!("Decoded {decoded} Ogg files");
    Ok(())
}
