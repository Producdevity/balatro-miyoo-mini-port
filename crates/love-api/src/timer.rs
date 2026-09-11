// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;
use std::sync::Arc;

use crate::state::SharedState;

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let timer = lua.create_table()?;

    // love.timer.getTime() -> seconds since start
    {
        let s = Arc::clone(&state);
        timer.set(
            "getTime",
            lua.create_function(move |_, ()| Ok(s.start_time.elapsed().as_secs_f64()))?,
        )?;
    }

    // love.timer.step() -> dt since last call
    {
        let s = Arc::clone(&state);
        timer.set(
            "step",
            lua.create_function(move |_, ()| {
                let now = std::time::Instant::now();
                let mut last = s.last_step_time.lock();
                let dt = last.elapsed().as_secs_f64();
                *last = now;
                *s.last_dt.lock() = dt as f32;
                Ok(dt)
            })?,
        )?;
    }

    // love.timer.getDelta() -> last dt
    {
        let s = Arc::clone(&state);
        timer.set(
            "getDelta",
            lua.create_function(move |_, ()| Ok(*s.last_dt.lock() as f64))?,
        )?;
    }

    // love.timer.getFPS() -> current FPS
    {
        let s = Arc::clone(&state);
        timer.set(
            "getFPS",
            lua.create_function(move |_, ()| Ok(s.fps_counter.lock().current_fps as i32))?,
        )?;
    }

    // love.timer.sleep(seconds) — reduced to minimal sleep to avoid double-sleeping
    // with the Rust frame limiter. Balatro's love.run() already calls sleep for FPS cap.
    timer.set(
        "sleep",
        lua.create_function(|_, _seconds: f64| {
            // Noop — Rust frame limiter handles timing
            Ok(())
        })?,
    )?;

    love.set("timer", timer)?;
    Ok(())
}
