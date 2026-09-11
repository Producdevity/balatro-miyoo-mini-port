use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const WINDOW_SIZE: usize = 120;
const FRAME_BUDGET_MS: f64 = 1000.0 / 30.0;

pub struct FrameBenchmark {
    samples: Option<Vec<Sample>>,
    state_names: BTreeMap<i64, String>,
    previous_frame: Option<Instant>,
}

struct Sample {
    elapsed_ms: f64,
    state: Option<i64>,
    interval_ms: Option<f64>,
}

impl FrameBenchmark {
    pub fn from_env() -> Self {
        let enabled = std::env::var("BALATRO_BENCHMARK").as_deref() == Ok("1");
        Self::new(enabled)
    }

    fn new(enabled: bool) -> Self {
        Self {
            samples: enabled.then(Vec::new),
            state_names: BTreeMap::new(),
            previous_frame: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.samples.is_some()
    }

    pub fn set_state_names(&mut self, state_names: BTreeMap<i64, String>) {
        self.state_names = state_names;
    }

    pub fn record(&mut self, elapsed: Duration, state: Option<i64>) {
        if let Some(samples) = &mut self.samples {
            let now = Instant::now();
            let interval_ms = self
                .previous_frame
                .replace(now)
                .map(|previous| now.duration_since(previous).as_secs_f64() * 1000.0);
            samples.push(Sample {
                elapsed_ms: elapsed.as_secs_f64() * 1000.0,
                state,
                interval_ms,
            });
        }
    }

    pub fn report(&self) {
        let Some(samples) = &self.samples else {
            return;
        };

        for (window, chunk) in samples.chunks(WINDOW_SIZE).enumerate() {
            if chunk.len() < WINDOW_SIZE {
                break;
            }
            let first_frame = window * WINDOW_SIZE + 1;
            let last_frame = first_frame + WINDOW_SIZE - 1;
            let elapsed: Vec<_> = chunk.iter().map(|sample| sample.elapsed_ms).collect();
            let summary = Summary::from_samples(&elapsed);
            eprintln!(
                "[benchmark] F={first_frame}-{last_frame} avg={:.1}ms median={:.1}ms p95={:.1}ms max={:.1}ms over-budget={:.1}%",
                summary.average_ms,
                summary.median_ms,
                summary.p95_ms,
                summary.max_ms,
                summary.over_budget_percent,
            );
        }

        let mut states = BTreeMap::<i64, Vec<f64>>::new();
        let mut cadence = BTreeMap::<i64, Vec<f64>>::new();
        for sample in samples {
            if let Some(state) = sample.state {
                states.entry(state).or_default().push(sample.elapsed_ms);
                if let Some(interval) = sample.interval_ms {
                    cadence.entry(state).or_default().push(interval);
                }
            }
        }
        for (state, elapsed) in states {
            let summary = Summary::from_samples(&elapsed);
            let name = self
                .state_names
                .get(&state)
                .map(String::as_str)
                .unwrap_or("UNKNOWN");
            eprintln!(
                "[benchmark-state] state={name}({state}) frames={} avg={:.1}ms median={:.1}ms p95={:.1}ms max={:.1}ms over-budget={:.1}%",
                elapsed.len(),
                summary.average_ms,
                summary.median_ms,
                summary.p95_ms,
                summary.max_ms,
                summary.over_budget_percent,
            );
            if let Some(intervals) = cadence.get(&state) {
                let cadence = Summary::from_samples(intervals);
                eprintln!(
                    "[frame-cadence] state={name} frames={} avg={:.1}ms p95={:.1}ms fps={:.2}",
                    intervals.len(),
                    cadence.average_ms,
                    cadence.p95_ms,
                    1000.0 / cadence.average_ms
                );
            }
        }
    }
}

struct Summary {
    average_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    over_budget_percent: f64,
}

impl Summary {
    fn from_samples(samples: &[f64]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let count = sorted.len();
        let percentile =
            |value: f64| sorted[((count as f64 * value).ceil() as usize - 1).min(count - 1)];
        let over_budget = samples
            .iter()
            .filter(|sample| **sample > FRAME_BUDGET_MS)
            .count();

        Self {
            average_ms: samples.iter().sum::<f64>() / count as f64,
            median_ms: percentile(0.5),
            p95_ms: percentile(0.95),
            max_ms: sorted[count - 1],
            over_budget_percent: over_budget as f64 * 100.0 / count as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameBenchmark, Summary, FRAME_BUDGET_MS};
    use std::time::Duration;

    #[test]
    fn disabled_benchmark_does_not_collect_samples() {
        let mut benchmark = FrameBenchmark::new(false);
        benchmark.record(Duration::from_millis(40), Some(1));
        assert!(benchmark.samples.is_none());
    }

    #[test]
    fn summary_reports_frame_budget_distribution() {
        let samples = [10.0, 20.0, 30.0, 40.0, 50.0];
        let summary = Summary::from_samples(&samples);
        assert_eq!(summary.average_ms, 30.0);
        assert_eq!(summary.median_ms, 30.0);
        assert_eq!(summary.p95_ms, 50.0);
        assert_eq!(summary.max_ms, 50.0);
        assert_eq!(summary.over_budget_percent, 40.0);
        assert!(FRAME_BUDGET_MS > 33.3);
    }
}
