use std::time::{Duration, Instant};

pub(super) struct AudioMetrics {
    enabled: bool,
    started: Instant,
    blocks: usize,
    late: usize,
    work: Duration,
    commands: Duration,
    decode: Duration,
    maximum: Duration,
    voices: usize,
    silent_streams: usize,
    subnormal_volumes: usize,
}

impl AudioMetrics {
    pub(super) fn record_starts(&self, voices: &[super::Voice], output_frame: u64) {
        if !self.enabled {
            return;
        }
        for voice in voices {
            if voice.position == 0.0
                && matches!(
                    voice.playback,
                    super::Playback::Pcm(_) | super::Playback::Stream(_)
                )
            {
                eprintln!(
                    "[audio-start] source={} output_frame={output_frame}",
                    voice.id
                );
            }
        }
    }

    pub(super) fn new() -> Self {
        Self {
            enabled: std::env::var("BALATRO_AUDIO_STATS").as_deref() == Ok("1"),
            started: Instant::now(),
            blocks: 0,
            late: 0,
            work: Duration::ZERO,
            commands: Duration::ZERO,
            decode: Duration::ZERO,
            maximum: Duration::ZERO,
            voices: 0,
            silent_streams: 0,
            subnormal_volumes: 0,
        }
    }

    pub(super) fn record(
        &mut self,
        work: Duration,
        commands: Duration,
        decode: Duration,
        voices: usize,
        silent_streams: usize,
        subnormal_volumes: usize,
    ) {
        if !self.enabled {
            return;
        }
        self.blocks += 1;
        self.work += work;
        self.commands += commands;
        self.decode += decode;
        self.maximum = self.maximum.max(work);
        self.voices = self.voices.max(voices);
        self.silent_streams = self.silent_streams.max(silent_streams);
        self.subnormal_volumes = self.subnormal_volumes.max(subnormal_volumes);
        let budget = super::MIX_FRAMES as f64 / super::OUTPUT_RATE as f64;
        self.late += usize::from(work.as_secs_f64() > budget);
        if self.started.elapsed().as_secs() >= 5 {
            eprintln!("[audio-stats] blocks={} work_avg_ms={:.3} work_max_ms={:.3} over_budget={} peak_voices={} peak_silent_streams={} command_avg_ms={:.3} peak_subnormal_volumes={} decode_avg_ms={:.3}",
                self.blocks, self.work.as_secs_f64()*1000.0/self.blocks as f64,
                self.maximum.as_secs_f64()*1000.0, self.late, self.voices, self.silent_streams,
                self.commands.as_secs_f64()*1000.0/self.blocks as f64, self.subnormal_volumes,
                self.decode.as_secs_f64()*1000.0/self.blocks as f64);
            self.started = Instant::now();
            self.blocks = 0;
            self.late = 0;
            self.work = Duration::ZERO;
            self.commands = Duration::ZERO;
            self.decode = Duration::ZERO;
            self.maximum = Duration::ZERO;
            self.voices = 0;
            self.silent_streams = 0;
            self.subnormal_volumes = 0;
        }
    }
}
