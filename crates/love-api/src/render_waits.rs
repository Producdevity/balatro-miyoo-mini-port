use std::time::Duration;

const CAPACITY: usize = 32;

#[derive(Clone, Copy, Default)]
struct Entry {
    file: &'static str,
    line: u32,
    calls: u64,
    elapsed: Duration,
    longest: Duration,
}

#[derive(Default)]
pub(crate) struct RenderWaits {
    entries: [Entry; CAPACITY],
    dropped: u64,
}

impl RenderWaits {
    pub(crate) fn record(&mut self, file: &'static str, line: u32, elapsed: Duration) {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.calls == 0 || (entry.file == file && entry.line == line));
        if let Some(entry) = entry {
            entry.file = file;
            entry.line = line;
            entry.calls += 1;
            entry.elapsed += elapsed;
            entry.longest = entry.longest.max(elapsed);
        } else {
            self.dropped += 1;
        }
    }

    pub(crate) fn report(&self) {
        eprintln!("[raster-wait] dropped={}", self.dropped);
        for entry in self.entries.iter().filter(|entry| entry.calls > 0) {
            eprintln!(
                "[raster-wait] {}:{} calls={} total_ms={:.3} max_ms={:.3}",
                entry.file,
                entry.line,
                entry.calls,
                entry.elapsed.as_secs_f64() * 1000.0,
                entry.longest.as_secs_f64() * 1000.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_are_grouped_by_file_and_line() {
        let mut waits = RenderWaits::default();
        waits.record("draw.rs", 7, Duration::from_millis(3));
        waits.record("draw.rs", 7, Duration::from_millis(5));
        waits.record("draw.rs", 8, Duration::from_millis(2));
        waits.record("present.rs", 7, Duration::from_millis(1));
        assert_eq!(waits.entries[0].calls, 2);
        assert_eq!(waits.entries[0].elapsed, Duration::from_millis(8));
        assert_eq!(waits.entries[0].longest, Duration::from_millis(5));
        assert_eq!(waits.entries[1].calls, 1);
        assert_eq!(waits.entries[2].calls, 1);
        assert_eq!(waits.dropped, 0);
    }

    #[test]
    fn a_full_report_keeps_existing_callers() {
        let mut waits = RenderWaits::default();
        for line in 0..CAPACITY as u32 + 2 {
            waits.record("draw.rs", line, Duration::from_millis(1));
        }
        waits.record("draw.rs", 0, Duration::from_millis(4));
        assert_eq!(waits.dropped, 2);
        assert_eq!(waits.entries[0].calls, 2);
        assert_eq!(waits.entries[0].elapsed, Duration::from_millis(5));
        assert_eq!(waits.entries[CAPACITY - 1].calls, 1);
    }
}
