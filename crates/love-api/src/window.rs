// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;
use std::sync::Arc;

use crate::state::SharedState;

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let win = lua.create_table()?;

    // love.window.getMode() -> width, height, flags
    {
        let s = Arc::clone(&state);
        win.set(
            "getMode",
            lua.create_function(move |lua, ()| {
                let w = *s.canvas_width.lock();
                let h = *s.canvas_height.lock();
                let flags = lua.create_table()?;
                flags.set("fullscreen", true)?;
                flags.set("fullscreentype", "desktop")?;
                flags.set("vsync", 0i32)?;
                flags.set("msaa", 0i32)?;
                flags.set("resizable", false)?;
                flags.set("borderless", false)?;
                flags.set("centered", true)?;
                flags.set("display", 1i32)?;
                flags.set("minwidth", 1i32)?;
                flags.set("minheight", 1i32)?;
                Ok((w, h, flags))
            })?,
        )?;
    }

    // love.window.setMode(w, h, flags) -> true
    {
        let s = Arc::clone(&state);
        win.set(
            "setMode",
            lua.create_function(move |_, _args: LuaMultiValue| {
                let w = *s.canvas_width.lock();
                let h = *s.canvas_height.lock();
                s.event_queue
                    .lock()
                    .push_back(crate::state::LoveEvent::Resize { w, h });
                Ok(true)
            })?,
        )?;
    }

    // love.window.updateMode(w, h, settings)
    // In real LÖVE, changing mode triggers a resize event; we emulate that.
    {
        let s = Arc::clone(&state);
        win.set(
            "updateMode",
            lua.create_function(move |_, _args: LuaMultiValue| {
                let w = *s.canvas_width.lock();
                let h = *s.canvas_height.lock();
                s.event_queue
                    .lock()
                    .push_back(crate::state::LoveEvent::Resize { w, h });
                Ok(true)
            })?,
        )?;
    }

    // love.window.isOpen()
    win.set("isOpen", lua.create_function(|_, ()| Ok(true))?)?;

    // love.window.getTitle()
    win.set("getTitle", lua.create_function(|_, ()| Ok("Balatro TUI"))?)?;

    // love.window.setTitle(title)
    win.set("setTitle", lua.create_function(|_, _title: String| Ok(()))?)?;

    // love.window.toPixels(value) -> value (no DPI scaling)
    win.set("toPixels", lua.create_function(|_, v: f64| Ok(v))?)?;

    // love.window.getDisplayCount()
    win.set("getDisplayCount", lua.create_function(|_, ()| Ok(1i32))?)?;

    // love.window.getDesktopDimensions(display)
    {
        let s = Arc::clone(&state);
        win.set(
            "getDesktopDimensions",
            lua.create_function(move |_, _display: Option<i32>| {
                let w = *s.canvas_width.lock();
                let h = *s.canvas_height.lock();
                Ok((w, h))
            })?,
        )?;
    }

    // love.window.getFullscreenModes(display)
    {
        let s = Arc::clone(&state);
        win.set(
            "getFullscreenModes",
            lua.create_function(move |lua, _display: Option<i32>| {
                let w = *s.canvas_width.lock();
                let h = *s.canvas_height.lock();
                let t = lua.create_table()?;
                let mode = lua.create_table()?;
                mode.set("width", w)?;
                mode.set("height", h)?;
                t.set(1, mode)?;
                Ok(t)
            })?,
        )?;
    }

    // love.window.showMessageBox
    win.set(
        "showMessageBox",
        lua.create_function(|_, (title, msg, _buttons): (String, String, LuaValue)| {
            eprintln!("[LOVE MessageBox] {}: {}", title, msg);
            Ok(1i32)
        })?,
    )?;

    // love.window.setIcon — noop
    win.set(
        "setIcon",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;

    // love.window.hasFocus
    win.set("hasFocus", lua.create_function(|_, ()| Ok(true))?)?;

    // love.window.isVisible
    win.set("isVisible", lua.create_function(|_, ()| Ok(true))?)?;

    // love.window.getDPIScale
    win.set("getDPIScale", lua.create_function(|_, ()| Ok(1.0f64))?)?;

    // love.window.getWidth
    {
        let s = Arc::clone(&state);
        win.set(
            "getWidth",
            lua.create_function(move |_, ()| Ok(*s.canvas_width.lock()))?,
        )?;
    }

    // love.window.getHeight
    {
        let s = Arc::clone(&state);
        win.set(
            "getHeight",
            lua.create_function(move |_, ()| Ok(*s.canvas_height.lock()))?,
        )?;
    }

    love.set("window", win)?;
    Ok(())
}
