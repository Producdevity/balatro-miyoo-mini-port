// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;
use std::sync::Arc;

use crate::state::{LoveEvent, SharedState};

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let event = lua.create_table()?;
    let use_terminal_input = std::env::var("BALATRO_PLATFORM").as_deref() != Ok("miyoo");

    // love.event.pump() — collect crossterm events into queue
    {
        let s = Arc::clone(&state);
        event.set(
            "pump",
            lua.create_function(move |_, ()| {
                if !use_terminal_input {
                    return Ok(());
                }
                use crossterm::event::{poll as ct_poll, read as ct_read, Event, KeyEventKind};
                use std::time::Duration;

                while ct_poll(Duration::from_millis(0)).unwrap_or(false) {
                    match ct_read() {
                        Ok(Event::Key(key_event)) => {
                            let key_str = keycode_to_love_string(key_event.code);
                            if key_str == "unknown" {
                                continue;
                            }
                            match key_event.kind {
                                KeyEventKind::Press => {
                                    s.keys_down.write().insert(key_str.clone());
                                    s.event_queue.lock().push_back(LoveEvent::KeyPressed {
                                        key: key_str.clone(),
                                        scancode: key_str.clone(),
                                        is_repeat: false,
                                    });
                                    // Fire textinput for printable characters
                                    if let crossterm::event::KeyCode::Char(c) = key_event.code {
                                        // Only fire for actual characters, not when Ctrl is held
                                        if !key_event
                                            .modifiers
                                            .contains(crossterm::event::KeyModifiers::CONTROL)
                                        {
                                            s.event_queue.lock().push_back(LoveEvent::TextInput {
                                                text: c.to_string(),
                                            });
                                        }
                                    }
                                }
                                KeyEventKind::Release => {
                                    s.keys_down.write().remove(&key_str);
                                    s.event_queue.lock().push_back(LoveEvent::KeyReleased {
                                        key: key_str.clone(),
                                        scancode: key_str,
                                    });
                                }
                                _ => {}
                            }
                        }
                        Ok(Event::Mouse(mouse_event)) => {
                            use crossterm::event::MouseEventKind;
                            // Map terminal coordinates to canvas coordinates
                            let term_cols = *s.terminal_cols.lock() as f32;
                            let term_rows = *s.terminal_rows.lock() as f32;
                            let canvas_w = *s.canvas_width.lock() as f32;
                            let canvas_h = *s.canvas_height.lock() as f32;
                            let mx = (mouse_event.column as f32 + 0.5) / term_cols * canvas_w;
                            let my = (mouse_event.row as f32 + 0.5) / term_rows * canvas_h;

                            match mouse_event.kind {
                                MouseEventKind::Down(btn) => {
                                    let button = match btn {
                                        crossterm::event::MouseButton::Left => 1u8,
                                        crossterm::event::MouseButton::Right => 2,
                                        crossterm::event::MouseButton::Middle => 3,
                                    };
                                    *s.mouse_x.lock() = mx;
                                    *s.mouse_y.lock() = my;
                                    s.mouse_buttons_down.write().insert(button);
                                    s.event_queue.lock().push_back(LoveEvent::MousePressed {
                                        x: mx,
                                        y: my,
                                        button,
                                        is_touch: false,
                                    });
                                }
                                MouseEventKind::Up(btn) => {
                                    let button = match btn {
                                        crossterm::event::MouseButton::Left => 1u8,
                                        crossterm::event::MouseButton::Right => 2,
                                        crossterm::event::MouseButton::Middle => 3,
                                    };
                                    *s.mouse_x.lock() = mx;
                                    *s.mouse_y.lock() = my;
                                    s.mouse_buttons_down.write().remove(&button);
                                    s.event_queue.lock().push_back(LoveEvent::MouseReleased {
                                        x: mx,
                                        y: my,
                                        button,
                                    });
                                }
                                MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                                    let old_x = *s.mouse_x.lock();
                                    let old_y = *s.mouse_y.lock();
                                    *s.mouse_x.lock() = mx;
                                    *s.mouse_y.lock() = my;
                                    s.event_queue.lock().push_back(LoveEvent::MouseMoved {
                                        x: mx,
                                        y: my,
                                        dx: mx - old_x,
                                        dy: my - old_y,
                                    });
                                }
                                _ => {}
                            }
                        }
                        Ok(Event::Resize(cols, rows)) => {
                            *s.terminal_cols.lock() = cols;
                            *s.terminal_rows.lock() = rows;
                            // Recalculate canvas proportionally to new terminal size.
                            // Formula depends on render mode (must match runner.rs).
                            let scale = s.canvas_scale;
                            let (new_w, new_h) = match s.render_mode {
                                0 => {
                                    // Octant: cols*2 × rows*4
                                    ((cols as u32 * 2).min(800), (rows as u32 * 4).min(600))
                                }
                                2 => {
                                    // Sixel: on Windows, Win32 GetClientRect (debounced) is
                                    // authoritative for sixel_text_w/h. crossterm may report
                                    // stale/wrong cols/rows — do NOT overwrite sixel target here.
                                    // Just recompute canvas from the current sixel target.
                                    let cur_tw = *s.sixel_text_w.lock();
                                    let cur_th = *s.sixel_text_h.lock();
                                    if cur_tw > 0 && cur_th > 0 {
                                        let bgt = 250_000u64;
                                        let act = cur_tw as u64 * cur_th as u64;
                                        if act <= bgt {
                                            (cur_tw, cur_th)
                                        } else {
                                            let sc = (act as f64 / bgt as f64).sqrt();
                                            (
                                                ((cur_tw as f64 / sc).round() as u32).max(320),
                                                ((cur_th as f64 / sc).round() as u32).max(180),
                                            )
                                        }
                                    } else {
                                        // WT VT340 Sixel: virtual cell = 10×20
                                        let px_w = cols as u32 * 10;
                                        let px_h = rows as u32 * 20;
                                        *s.sixel_text_w.lock() = px_w;
                                        *s.sixel_text_h.lock() = px_h;
                                        eprintln!(
                                            "[RESIZE] {}x{} cells → sixel_target={}x{}",
                                            cols, rows, px_w, px_h
                                        );
                                        let bgt = 250_000u64;
                                        let act = px_w as u64 * px_h as u64;
                                        if act <= bgt {
                                            (px_w, px_h)
                                        } else {
                                            let sc = (act as f64 / bgt as f64).sqrt();
                                            (
                                                ((px_w as f64 / sc).round() as u32).max(320),
                                                ((px_h as f64 / sc).round() as u32).max(180),
                                            )
                                        }
                                    }
                                }
                                _ => {
                                    // HalfBlock: cols*scale × rows*2*scale
                                    (
                                        (cols as u32 * scale).min(800),
                                        (rows as u32 * 2 * scale).min(600),
                                    )
                                }
                            };
                            *s.canvas_width.lock() = new_w;
                            *s.canvas_height.lock() = new_h;
                            s.flush_render_jobs();
                            s.pixel_buffer.lock().resize(new_w, new_h);
                            s.event_queue
                                .lock()
                                .push_back(LoveEvent::Resize { w: new_w, h: new_h });
                        }
                        Ok(Event::FocusGained) => {
                            s.event_queue.lock().push_back(LoveEvent::Focus(true));
                            s.event_queue.lock().push_back(LoveEvent::Visible(true));
                        }
                        Ok(Event::FocusLost) => {
                            s.event_queue.lock().push_back(LoveEvent::Focus(false));
                        }
                        _ => {}
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // love.event.poll() — returns iterator function
    {
        let s = Arc::clone(&state);
        event.set(
            "poll",
            lua.create_function(move |lua, ()| {
                let s2 = Arc::clone(&s);
                let iter = lua.create_function(move |lua, ()| {
                    let mut queue = s2.event_queue.lock();
                    match queue.pop_front() {
                        None => Ok(LuaMultiValue::new()),
                        Some(ev) => love_event_to_lua_values(lua, ev),
                    }
                })?;
                Ok(iter)
            })?,
        )?;
    }

    // love.event.quit([exitstatus])
    {
        let s = Arc::clone(&state);
        event.set(
            "quit",
            lua.create_function(move |_, code: Option<i32>| {
                *s.should_quit.lock() = true;
                s.event_queue
                    .lock()
                    .push_back(LoveEvent::Quit(code.unwrap_or(0)));
                Ok(())
            })?,
        )?;
    }

    // love.event.push(name, ...) — generic event push
    {
        let s = Arc::clone(&state);
        event.set(
            "push",
            lua.create_function(move |_, args: LuaMultiValue| {
                if let Some(LuaValue::String(name)) = args.get(0) {
                    let name_str = name.to_string_lossy();
                    if name_str == "quit" {
                        *s.should_quit.lock() = true;
                        s.event_queue.lock().push_back(LoveEvent::Quit(0));
                    }
                }
                Ok(())
            })?,
        )?;
    }

    love.set("event", event)?;

    // Fire initial focus/visible events so the game knows it has focus
    state.event_queue.lock().push_back(LoveEvent::Focus(true));
    state.event_queue.lock().push_back(LoveEvent::Visible(true));

    // Set up love.handlers table
    setup_handlers(lua)?;

    Ok(())
}

fn setup_handlers(lua: &Lua) -> LuaResult<()> {
    lua.load(
        r#"
        love.handlers = love.handlers or {}
        love.handlers.keypressed = function(a,b,c,d,e,f)
            if love.keypressed then love.keypressed(a,b,c) end
        end
        love.handlers.keyreleased = function(a,b,c,d,e,f)
            if love.keyreleased then love.keyreleased(a,b) end
        end
        love.handlers.mousepressed = function(a,b,c,d,e,f)
            if love.mousepressed then love.mousepressed(a,b,c,d) end
        end
        love.handlers.mousereleased = function(a,b,c,d,e,f)
            if love.mousereleased then love.mousereleased(a,b,c) end
        end
        love.handlers.mousemoved = function(a,b,c,d,e,f)
            if love.mousemoved then love.mousemoved(a,b,c,d) end
        end
        love.handlers.resize = function(a,b,c,d,e,f)
            if love.resize then love.resize(a,b) end
        end
        love.handlers.quit = function(a,b,c,d,e,f)
            if love.quit then return love.quit() end
        end
        love.handlers.focus = function(a)
            if love.focus then love.focus(a) end
        end
        love.handlers.visible = function(a)
            if love.visible then love.visible(a) end
        end
        love.handlers.gamepadpressed = function(a,b)
            if love.gamepadpressed then love.gamepadpressed(a,b) end
        end
        love.handlers.gamepadreleased = function(a,b)
            if love.gamepadreleased then love.gamepadreleased(a,b) end
        end
        love.handlers.joystickaxis = function(a,b,c)
            if love.joystickaxis then love.joystickaxis(a,b,c) end
        end
        love.handlers.textinput = function(a)
            if love.textinput then love.textinput(a) end
        end
    "#,
    )
    .exec()?;
    Ok(())
}

fn love_event_to_lua_values(lua: &Lua, ev: LoveEvent) -> LuaResult<LuaMultiValue> {
    match ev {
        LoveEvent::Quit(code) => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("quit")?),
            LuaValue::Integer(code as _),
        ])),
        LoveEvent::KeyPressed {
            key,
            scancode,
            is_repeat,
        } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("keypressed")?),
            LuaValue::String(lua.create_string(&key)?),
            LuaValue::String(lua.create_string(&scancode)?),
            LuaValue::Boolean(is_repeat),
        ])),
        LoveEvent::KeyReleased { key, scancode } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("keyreleased")?),
            LuaValue::String(lua.create_string(&key)?),
            LuaValue::String(lua.create_string(&scancode)?),
        ])),
        LoveEvent::MousePressed {
            x,
            y,
            button,
            is_touch,
        } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("mousepressed")?),
            LuaValue::Number(x as f64),
            LuaValue::Number(y as f64),
            LuaValue::Integer(button as _),
            LuaValue::Boolean(is_touch),
        ])),
        LoveEvent::MouseReleased { x, y, button } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("mousereleased")?),
            LuaValue::Number(x as f64),
            LuaValue::Number(y as f64),
            LuaValue::Integer(button as _),
        ])),
        LoveEvent::MouseMoved { x, y, dx, dy } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("mousemoved")?),
            LuaValue::Number(x as f64),
            LuaValue::Number(y as f64),
            LuaValue::Number(dx as f64),
            LuaValue::Number(dy as f64),
        ])),
        LoveEvent::Resize { w, h } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("resize")?),
            LuaValue::Integer(w as _),
            LuaValue::Integer(h as _),
        ])),
        LoveEvent::TextInput { text } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("textinput")?),
            LuaValue::String(lua.create_string(&text)?),
        ])),
        LoveEvent::Focus(focused) => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("focus")?),
            LuaValue::Boolean(focused),
        ])),
        LoveEvent::Visible(visible) => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("visible")?),
            LuaValue::Boolean(visible),
        ])),
        LoveEvent::GamepadPressed { joystick, button } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("gamepadpressed")?),
            LuaValue::Integer(joystick as _),
            LuaValue::String(lua.create_string(&button)?),
        ])),
        LoveEvent::GamepadReleased { joystick, button } => Ok(LuaMultiValue::from_vec(vec![
            LuaValue::String(lua.create_string("gamepadreleased")?),
            LuaValue::Integer(joystick as _),
            LuaValue::String(lua.create_string(&button)?),
        ])),
    }
}

fn keycode_to_love_string(code: crossterm::event::KeyCode) -> String {
    use crossterm::event::KeyCode::*;
    match code {
        Char(' ') => "space".to_string(),
        Char(c) => c.to_lowercase().to_string(),
        Enter => "return".to_string(),
        Backspace => "backspace".to_string(),
        Left => "left".to_string(),
        Right => "right".to_string(),
        Up => "up".to_string(),
        Down => "down".to_string(),
        Esc => "escape".to_string(),
        Tab => "tab".to_string(),
        Delete => "delete".to_string(),
        Insert => "insert".to_string(),
        Home => "home".to_string(),
        End => "end".to_string(),
        PageUp => "pageup".to_string(),
        PageDown => "pagedown".to_string(),
        F(n) => format!("f{}", n),
        CapsLock => "capslock".to_string(),
        Modifier(m) => match m {
            crossterm::event::ModifierKeyCode::LeftShift => "lshift".to_string(),
            crossterm::event::ModifierKeyCode::RightShift => "rshift".to_string(),
            crossterm::event::ModifierKeyCode::LeftControl => "lctrl".to_string(),
            crossterm::event::ModifierKeyCode::RightControl => "rctrl".to_string(),
            crossterm::event::ModifierKeyCode::LeftAlt => "lalt".to_string(),
            crossterm::event::ModifierKeyCode::RightAlt => "ralt".to_string(),
            _ => "unknown".to_string(),
        },
        _ => "unknown".to_string(),
    }
}
