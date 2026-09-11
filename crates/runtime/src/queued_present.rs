use anyhow::{Context, Result};
use love_api::state::SharedState;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct QueuedPresent {
    pending: Option<mpsc::Receiver<Result<()>>>,
    queued: u64,
    presented: u64,
}

impl QueuedPresent {
    pub fn submit(&mut self, state: &SharedState, effects: [f32; 2]) -> Result<Duration> {
        // The game may submit the next frame while this one renders, but never
        // accumulate more than one unacknowledged output boundary.
        let waited = self.finish()?;
        let (done, wait) = mpsc::channel();
        state.after_render(move |buffer| {
            buffer.apply_crt_effect(effects[0], effects[1]);
            let result = crate::platform::present(buffer);
            let _ = done.send(result);
        });
        self.pending = Some(wait);
        self.queued += 1;
        Ok(waited)
    }

    pub fn finish(&mut self) -> Result<Duration> {
        let start = Instant::now();
        if let Some(wait) = self.pending.take() {
            wait.recv()
                .context("raster worker stopped before presentation")??;
            self.presented += 1;
        }
        Ok(start.elapsed())
    }

    pub fn report(&self) {
        eprintln!(
            "[queued-present] queued={} presented={}",
            self.queued, self.presented
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reports_worker_failures_without_counting_a_frame() {
        let (done, wait) = mpsc::channel();
        let mut output = QueuedPresent {
            pending: Some(wait),
            queued: 1,
            presented: 0,
        };
        done.send(Err(anyhow::anyhow!("display failed"))).unwrap();
        assert!(output
            .finish()
            .unwrap_err()
            .to_string()
            .contains("display failed"));
        assert_eq!(output.presented, 0);
        output.finish().unwrap();
        assert_eq!(output.presented, 0);
    }

    #[test]
    fn finish_consumes_each_acknowledgement_once() {
        let (done, wait) = mpsc::channel();
        let mut output = QueuedPresent {
            pending: Some(wait),
            queued: 1,
            presented: 0,
        };
        done.send(Ok(())).unwrap();
        output.finish().unwrap();
        output.finish().unwrap();
        assert_eq!(output.presented, 1);
        let (done, wait) = mpsc::channel();
        output.pending = Some(wait);
        drop(done);
        assert!(output.finish().is_err());
    }
}
