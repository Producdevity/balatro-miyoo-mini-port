use anyhow::Context;
use anyhow::Result;
use std::io::Write;
use std::time::{Duration, Instant};

pub(super) enum AudioOutput {
    Device(DeviceOutput),
    Capture(CaptureOutput),
    Onion(super::onion::OnionOutput),
}

impl AudioOutput {
    pub(super) fn open() -> Result<Self> {
        if let Some(path) = std::env::var_os("BALATRO_AUDIO_CAPTURE") {
            return CaptureOutput::open(std::path::Path::new(&path)).map(Self::Capture);
        }
        if std::env::var("BALATRO_PLATFORM").as_deref() == Ok("miyoo") {
            return super::onion::OnionOutput::open().map(Self::Onion);
        }
        DeviceOutput::open().map(Self::Device)
    }

    pub(super) fn write_samples(&mut self, samples: &[i16]) -> Result<()> {
        match self {
            Self::Device(output) => output.write_samples(samples),
            Self::Capture(output) => output.write_samples(samples),
            Self::Onion(output) => output.write_samples(samples),
        }
    }
}

pub(super) struct CaptureOutput {
    file: std::fs::File,
    next_write: Instant,
    samples_written: usize,
}

impl CaptureOutput {
    fn open(path: &std::path::Path) -> Result<Self> {
        // Never fall back to the speaker if a diagnostic capture fails.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create audio capture {}", path.display()))?;
        eprintln!(
            "[audio] silent PCM capture: {} (s16le, stereo, 44100 Hz, maximum 120 seconds)",
            path.display()
        );
        Ok(Self {
            file,
            next_write: Instant::now(),
            samples_written: 0,
        })
    }

    fn write_samples(&mut self, samples: &[i16]) -> Result<()> {
        let limit = super::OUTPUT_RATE as usize * 2 * 120;
        let count = samples
            .len()
            .min(limit.saturating_sub(self.samples_written));
        write_pcm(&mut self.file, &samples[..count])?;
        self.samples_written += count;
        let period =
            Duration::from_secs_f64(samples.len() as f64 / (2.0 * super::OUTPUT_RATE as f64));
        self.next_write = (self.next_write + period).max(Instant::now());
        std::thread::sleep(self.next_write.saturating_duration_since(Instant::now()));
        Ok(())
    }
}

fn write_pcm(file: &mut std::fs::File, samples: &[i16]) -> Result<()> {
    let bytes = unsafe {
        std::slice::from_raw_parts(
            samples.as_ptr().cast::<u8>(),
            std::mem::size_of_val(samples),
        )
    };
    file.write_all(bytes)?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) struct DeviceOutput(std::fs::File);

#[cfg(target_os = "linux")]
impl DeviceOutput {
    pub(super) fn open() -> Result<Self> {
        use std::os::fd::AsRawFd;

        const SNDCTL_DSP_SPEED: u32 = 0xc004_5002;
        const SNDCTL_DSP_SETFMT: u32 = 0xc004_5005;
        const SNDCTL_DSP_CHANNELS: u32 = 0xc004_5006;
        const SNDCTL_DSP_SETFRAGMENT: u32 = 0xc004_500a;
        const AFMT_S16_LE: libc::c_int = 0x10;

        let path = std::env::var("BALATRO_AUDIO_DEVICE").unwrap_or_else(|_| "/dev/dsp".to_owned());
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .with_context(|| format!("open {path}"))?;
        let fd = file.as_raw_fd();
        let mut fragment: libc::c_int = (4 << 16) | 10;
        if unsafe { libc::ioctl(fd, SNDCTL_DSP_SETFRAGMENT as _, &mut fragment) } != 0 {
            eprintln!(
                "[audio] output buffer request declined: {}",
                std::io::Error::last_os_error()
            );
        }

        let mut format = AFMT_S16_LE;
        let mut channels: libc::c_int = 2;
        let mut rate: libc::c_int = super::OUTPUT_RATE as libc::c_int;
        for (name, request, value) in [
            ("format", SNDCTL_DSP_SETFMT, &mut format),
            ("channels", SNDCTL_DSP_CHANNELS, &mut channels),
            ("sample rate", SNDCTL_DSP_SPEED, &mut rate),
        ] {
            if unsafe { libc::ioctl(fd, request as _, value) } != 0 {
                return Err(std::io::Error::last_os_error()).with_context(|| name.to_owned());
            }
        }
        if format != AFMT_S16_LE || channels != 2 || rate != super::OUTPUT_RATE as libc::c_int {
            anyhow::bail!(
                "unsupported output format: format={format} channels={channels} rate={rate}"
            );
        }
        eprintln!("[audio] output device: {path}, {rate} Hz, {channels} channels");
        Ok(Self(file))
    }

    pub(super) fn write_samples(&mut self, samples: &[i16]) -> Result<()> {
        write_pcm(&mut self.0, samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preserves_pcm_and_refuses_to_overwrite() {
        let path =
            std::env::temp_dir().join(format!("balatro-pcm-test-{}.pcm", std::process::id()));
        let mut output = CaptureOutput::open(&path).unwrap();
        output.write_samples(&[0, -32768, 32767, -1]).unwrap();
        assert!(CaptureOutput::open(&path).is_err());
        drop(output);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(bytes, [0, 0, 0, 128, 255, 127, 255, 255]);
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) struct DeviceOutput;

#[cfg(not(target_os = "linux"))]
impl DeviceOutput {
    pub(super) fn open() -> Result<Self> {
        anyhow::bail!("native audio output is only available on Linux")
    }

    pub(super) fn write_samples(&mut self, _samples: &[i16]) -> Result<()> {
        Ok(())
    }
}
