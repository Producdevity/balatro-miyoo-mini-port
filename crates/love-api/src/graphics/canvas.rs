// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newCanvas([w, h, settings]) -> Canvas
    {
        let s = Arc::clone(state);
        g.set(
            "newCanvas",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let w = match args.get(0) {
                    Some(LuaValue::Number(n)) => *n as u32,
                    Some(LuaValue::Integer(n)) => *n as u32,
                    _ => *s.canvas_width.lock(),
                };
                let h = match args.get(1) {
                    Some(LuaValue::Number(n)) => *n as u32,
                    Some(LuaValue::Integer(n)) => *n as u32,
                    _ => *s.canvas_height.lock(),
                };

                // Allocate a canvas ID and create the PixelBuffer
                let canvas_id = {
                    let mut id_lock = s.next_canvas_id.lock();
                    let id = *id_lock;
                    *id_lock += 1;
                    id
                };
                s.canvases.lock().insert(canvas_id, PixelBuffer::new(w, h));

                let c = lua.create_table()?;
                c.set("_canvas_id", canvas_id)?;
                c.set(
                    "setFilter",
                    lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
                )?;
                c.set(
                    "getFilter",
                    lua.create_function(|_, _self: LuaValue| Ok(("nearest", "nearest")))?,
                )?;
                c.set(
                    "getDimensions",
                    lua.create_function(move |_, _self: LuaValue| Ok((w, h)))?,
                )?;
                c.set(
                    "getWidth",
                    lua.create_function(move |_, _self: LuaValue| Ok(w))?,
                )?;
                c.set(
                    "getHeight",
                    lua.create_function(move |_, _self: LuaValue| Ok(h))?,
                )?;
                c.set(
                    "getPixelWidth",
                    lua.create_function(move |_, _self: LuaValue| Ok(w))?,
                )?;
                c.set(
                    "getPixelHeight",
                    lua.create_function(move |_, _self: LuaValue| Ok(h))?,
                )?;

                // renderTo: set canvas active, call function, restore
                {
                    let sr = Arc::clone(&s);
                    c.set(
                        "renderTo",
                        lua.create_function(
                            move |_, (self_tbl, func): (LuaTable, LuaFunction)| {
                                let cid: u64 = self_tbl.get("_canvas_id").unwrap_or(0);
                                let prev = *sr.active_canvas.lock();
                                *sr.active_canvas.lock() = cid;
                                let result = func.call::<()>(());
                                *sr.active_canvas.lock() = prev;
                                result?;
                                Ok(())
                            },
                        )?,
                    )?;
                }

                {
                    let sr = Arc::clone(&s);
                    c.set(
                        "release",
                        lua.create_function(move |_, self_tbl: LuaTable| {
                            let cid: u64 = self_tbl.get("_canvas_id").unwrap_or(0);
                            sr.canvases.lock().remove(&cid);
                            Ok(())
                        })?,
                    )?;
                }

                c.set(
                    "type",
                    lua.create_function(|_, _self: LuaValue| Ok("Canvas"))?,
                )?;
                c.set(
                    "typeOf",
                    lua.create_function(|_, (_self, t): (LuaValue, String)| {
                        Ok(t == "Canvas" || t == "Texture" || t == "Drawable" || t == "Object")
                    })?,
                )?;

                // Attach GC guard
                let guard = lua.create_userdata(ResourceGuard {
                    id: canvas_id,
                    kind: ResourceKind::Canvas,
                    state: Arc::clone(&s),
                })?;
                c.set("_gc_guard", guard)?;

                Ok(LuaValue::Table(c))
            })?,
        )?;
    }

    // love.graphics.setCanvas([canvas])
    {
        let s = Arc::clone(state);
        let queued_output = std::env::var("BALATRO_FRAME_PIPELINE").as_deref() == Ok("2");
        let registry_key = lua.create_registry_value(LuaNil)?;
        let key = Arc::new(Mutex::new(registry_key));
        let key2 = Arc::clone(&key);

        g.set(
            "setCanvas",
            lua.create_function(move |lua, args: LuaMultiValue| {
                if queued_output
                    && *s.active_canvas.lock() == 0
                    && matches!(args.front(), None | Some(LuaNil))
                {
                    return Ok(());
                }
                s.flush_render_jobs();
                match args.get(0) {
                    Some(LuaValue::Table(t)) => {
                        // Check for direct canvas (has _canvas_id)
                        let canvas_id = t.get::<u64>("_canvas_id").unwrap_or(0);
                        if canvas_id != 0 {
                            *s.active_canvas.lock() = canvas_id;
                            // Auto-clear on first activation this frame
                            if s.canvases_activated_this_frame.lock().insert(canvas_id) {
                                if let Some(cb) = s.canvases.lock().get_mut(&canvas_id) {
                                    cb.clear(0.0, 0.0, 0.0, 0.0);
                                    cb.clear_stencil();
                                }
                            }
                            let new_key = lua.create_registry_value(LuaValue::Table(t.clone()))?;
                            *key.lock() = new_key;
                        } else {
                            // Handle setCanvas{canvas} — table wrapping (LÖVE convention)
                            if let Ok(LuaValue::Table(inner)) = t.get::<LuaValue>(1) {
                                let inner_id = inner.get::<u64>("_canvas_id").unwrap_or(0);
                                if inner_id != 0 {
                                    *s.active_canvas.lock() = inner_id;
                                    // Auto-clear on first activation this frame
                                    if s.canvases_activated_this_frame.lock().insert(inner_id) {
                                        if let Some(cb) = s.canvases.lock().get_mut(&inner_id) {
                                            cb.clear(0.0, 0.0, 0.0, 0.0);
                                            cb.clear_stencil();
                                        }
                                    }
                                    let new_key =
                                        lua.create_registry_value(LuaValue::Table(inner.clone()))?;
                                    *key.lock() = new_key;
                                } else {
                                    *s.active_canvas.lock() = 0;
                                    let new_key = lua.create_registry_value(LuaNil)?;
                                    *key.lock() = new_key;
                                }
                            } else {
                                *s.active_canvas.lock() = 0;
                                let new_key = lua.create_registry_value(LuaNil)?;
                                *key.lock() = new_key;
                            }
                        }
                    }
                    _ => {
                        *s.active_canvas.lock() = 0;
                        let new_key = lua.create_registry_value(LuaNil)?;
                        *key.lock() = new_key;
                    }
                }
                Ok(())
            })?,
        )?;

        // love.graphics.getCanvas()
        g.set(
            "getCanvas",
            lua.create_function(move |lua, ()| {
                let val: LuaValue = lua.registry_value(&key2.lock())?;
                Ok(val)
            })?,
        )?;
    }

    Ok(())
}
