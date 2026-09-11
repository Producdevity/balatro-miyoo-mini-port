use mlua::Lua;
use std::time::{Duration, Instant};

// 32 KiB fell behind repeated deck-tab rebuilds under the 96 MiB device budget.
const DEFAULT_STEP_KIB: i32 = 128;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GcStats {
    pub steps: u64,
    pub cycles: u64,
    pub total: Duration,
    pub longest: Duration,
}

pub struct IncrementalGc {
    step_kib: i32,
    stats: GcStats,
}

impl IncrementalGc {
    pub fn from_env() -> Self {
        let step_kib = std::env::var("BALATRO_GC_STEP_KIB")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value >= 0)
            .unwrap_or(DEFAULT_STEP_KIB);
        Self {
            step_kib,
            stats: GcStats::default(),
        }
    }

    pub fn step_kib(&self) -> i32 {
        self.step_kib
    }

    pub fn step(&mut self, lua: &Lua) -> mlua::Result<()> {
        if self.step_kib == 0 {
            return Ok(());
        }

        let started = Instant::now();
        let completed = lua.gc_step_kbytes(self.step_kib)?;
        let elapsed = started.elapsed();
        self.stats.steps += 1;
        self.stats.cycles += u64::from(completed);
        self.stats.total += elapsed;
        self.stats.longest = self.stats.longest.max(elapsed);
        Ok(())
    }

    pub fn take_stats(&mut self) -> GcStats {
        std::mem::take(&mut self.stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_step_disables_manual_collection() {
        let lua = Lua::new();
        let mut gc = IncrementalGc {
            step_kib: 0,
            stats: GcStats::default(),
        };

        gc.step(&lua).unwrap();

        assert_eq!(gc.take_stats(), GcStats::default());
    }

    #[test]
    fn stats_are_reported_per_window() {
        let lua = Lua::new();
        let mut gc = IncrementalGc {
            step_kib: 1,
            stats: GcStats::default(),
        };

        gc.step(&lua).unwrap();
        let stats = gc.take_stats();
        assert_eq!(stats.steps, 1);
        assert!(stats.total >= stats.longest);
        assert_eq!(gc.take_stats(), GcStats::default());
    }
}
