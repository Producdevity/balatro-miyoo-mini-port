use mlua::{HookTriggers, Lua, VmState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const DEFAULT_REPORT_LIMIT: usize = 20;

#[derive(Default)]
struct Samples {
    total: u64,
    locations: HashMap<(String, i32), u64>,
}

pub struct LuaSampler {
    samples: Arc<Mutex<Samples>>,
    interval: u32,
    report_limit: usize,
}

impl LuaSampler {
    pub fn from_env(lua: &Lua) -> Option<Self> {
        let interval = std::env::var("BALATRO_LUA_SAMPLE_INTERVAL")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)?;

        let sampler = Self::install(lua, interval, DEFAULT_REPORT_LIMIT);
        eprintln!("[lua-sampler] interval={interval} instructions");
        Some(sampler)
    }

    fn install(lua: &Lua, interval: u32, report_limit: usize) -> Self {
        let samples = Arc::new(Mutex::new(Samples::default()));
        let sampler = Self {
            samples,
            interval,
            report_limit,
        };
        sampler.activate(lua);
        sampler
    }

    fn activate(&self, lua: &Lua) {
        let hook_samples = Arc::clone(&self.samples);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(self.interval),
            move |_lua, debug| {
                let source = debug
                    .source()
                    .short_src
                    .map(|source| source.into_owned())
                    .unwrap_or_else(|| "?".to_owned());
                let line = debug.curr_line();
                let mut samples = hook_samples.lock().expect("Lua sampler lock poisoned");
                samples.total += 1;
                *samples.locations.entry((source, line)).or_default() += 1;
                Ok(VmState::Continue)
            },
        );
    }

    pub fn take_report(&self, lua: &Lua) -> String {
        let mut samples = self.samples.lock().expect("Lua sampler lock poisoned");
        let total = samples.total;
        let mut locations: Vec<_> = samples.locations.drain().collect();
        samples.total = 0;
        drop(samples);
        self.activate(lua);

        locations.sort_unstable_by(|left, right| {
            right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
        });
        let top = locations
            .into_iter()
            .take(self.report_limit)
            .map(|((source, line), count)| format!("{source}:{line}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("total={total} top=[{top}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_and_resets_samples() {
        let lua = Lua::new();
        let sampler = LuaSampler::install(&lua, 100, 4);
        lua.load("local n = 0; for i = 1, 50000 do n = n + i end")
            .set_name("@sample_target.lua")
            .exec()
            .unwrap();

        let first = sampler.take_report(&lua);
        let second = sampler.take_report(&lua);

        assert!(first.starts_with("total="));
        assert!(first.contains("sample_target.lua:"));
        assert_eq!(second, "total=0 top=[]");
    }
}
