use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Cadence {
    started: Option<Instant>,
    frames: usize,
    max_mix: Duration,
    max_write: Duration,
}

pub(super) struct Report {
    pub frames_per_second: f64,
    pub max_mix: Duration,
    pub max_write: Duration,
}

impl Cadence {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn record(
        &mut self,
        start: Instant,
        mixed: Instant,
        written: Instant,
        frames: usize,
    ) -> Option<Report> {
        let started = *self.started.get_or_insert(start);
        self.frames += frames;
        self.max_mix = self.max_mix.max(mixed.duration_since(start));
        self.max_write = self.max_write.max(written.duration_since(mixed));
        let elapsed = written.duration_since(started);
        if elapsed < Duration::from_secs(10) {
            return None;
        }
        let report = Report {
            frames_per_second: self.frames as f64 / elapsed.as_secs_f64(),
            max_mix: self.max_mix,
            max_write: self.max_write,
        };
        self.reset();
        self.started = Some(written);
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(cadence: &mut Cadence, at: Instant, duration: Duration) -> Option<Report> {
        cadence.record(at, at + Duration::from_millis(1), at + duration, 441)
    }

    #[test]
    fn reports_sample_cadence_and_separates_mixing_from_output_wait() {
        let start = Instant::now();
        let period = Duration::from_millis(10);
        let mut cadence = Cadence::default();
        for i in 0..999 {
            assert!(block(&mut cadence, start + period * i, period).is_none());
        }
        let report = block(&mut cadence, start + period * 999, period).unwrap();
        assert_eq!(report.frames_per_second, 44_100.0);
        assert_eq!(report.max_mix, Duration::from_millis(1));
        assert_eq!(report.max_write, Duration::from_millis(9));
        for i in 0..499 {
            assert!(block(
                &mut cadence,
                start + Duration::from_secs(10) + period * i * 2,
                period * 2
            )
            .is_none());
        }
        let report = block(
            &mut cadence,
            start + Duration::from_millis(19_980),
            period * 2,
        )
        .unwrap();
        assert_eq!(report.frames_per_second, 22_050.0);
    }

    #[test]
    fn deliberate_idle_time_is_not_reported_as_slow_playback() {
        let start = Instant::now();
        let mut cadence = Cadence::default();
        block(&mut cadence, start, Duration::from_millis(10));
        cadence.reset();
        assert!(block(
            &mut cadence,
            start + Duration::from_secs(60),
            Duration::from_millis(10)
        )
        .is_none());
    }
}
