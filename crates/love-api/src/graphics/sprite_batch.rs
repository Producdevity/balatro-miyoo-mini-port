// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newSpriteBatch(image [, maxsprites, usage]) -> SpriteBatch
    {
        let s = Arc::clone(state);
        g.set(
            "newSpriteBatch",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let image_id = match args.get(0) {
                    Some(LuaValue::Table(t)) => t.get::<u64>("_image_id").unwrap_or(0),
                    _ => 0,
                };
                let img_tbl = match args.get(0) {
                    Some(LuaValue::Table(t)) => t.clone(),
                    _ => lua.create_table()?,
                };

                // Create SpriteBatch in registry
                let sb_id = {
                    let mut next = s.next_spritebatch_id.lock();
                    let id = *next;
                    *next += 1;
                    id
                };
                s.sprite_batches.lock().insert(
                    sb_id,
                    crate::state::SpriteBatchData {
                        image_id,
                        entries: Vec::new(),
                        color: None,
                    },
                );

                let sb = lua.create_table()?;
                sb.set("_spritebatch_id", sb_id)?;
                sb.set("_image", img_tbl)?;

                // SpriteBatch:add([quad], x, y, r, sx, sy, ox, oy) -> id
                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "add",
                        lua.create_function(move |_, args: LuaMultiValue| {
                            let self_tbl = match args.get(0) {
                                Some(LuaValue::Table(t)) => t,
                                _ => return Ok(0i64),
                            };
                            let sb_id = self_tbl.get::<u64>("_spritebatch_id").unwrap_or(0);

                            let mut arg_idx = 1;
                            let (qx, qy, qw, qh) = match args.get(1) {
                                Some(LuaValue::Table(t)) => {
                                    match (
                                        t.get::<f32>("_x"),
                                        t.get::<f32>("_y"),
                                        t.get::<f32>("_w"),
                                        t.get::<f32>("_h"),
                                    ) {
                                        (Ok(x), Ok(y), Ok(w), Ok(h)) => {
                                            arg_idx = 2;
                                            (x, y, w, h)
                                        }
                                        _ => (0.0, 0.0, 0.0, 0.0),
                                    }
                                }
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };

                            let gf = |idx: usize, def: f32| -> f32 {
                                match args.get(idx) {
                                    Some(LuaValue::Number(n)) => *n as f32,
                                    Some(LuaValue::Integer(n)) => *n as f32,
                                    _ => def,
                                }
                            };

                            let entry = crate::state::SpriteBatchEntry {
                                quad_x: qx,
                                quad_y: qy,
                                quad_w: qw,
                                quad_h: qh,
                                x: gf(arg_idx, 0.0),
                                y: gf(arg_idx + 1, 0.0),
                                r: gf(arg_idx + 2, 0.0),
                                sx: gf(arg_idx + 3, 1.0),
                                sy: gf(arg_idx + 4, gf(arg_idx + 3, 1.0)),
                                ox: gf(arg_idx + 5, 0.0),
                                oy: gf(arg_idx + 6, 0.0),
                                color: None,
                            };

                            let mut sbs = sr.sprite_batches.lock();
                            if let Some(data) = sbs.get_mut(&sb_id) {
                                data.entries.push(entry);
                                Ok(data.entries.len() as i64)
                            } else {
                                Ok(0i64)
                            }
                        })?,
                    )?;
                }

                // SpriteBatch:set(id, [quad], x, y, r, sx, sy, ox, oy)
                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "set",
                        lua.create_function(move |_, args: LuaMultiValue| {
                            let self_tbl = match args.get(0) {
                                Some(LuaValue::Table(t)) => t,
                                _ => return Ok(()),
                            };
                            let sb_id = self_tbl.get::<u64>("_spritebatch_id").unwrap_or(0);
                            let entry_id = match args.get(1) {
                                Some(LuaValue::Integer(n)) => (*n as usize).saturating_sub(1),
                                Some(LuaValue::Number(n)) => (*n as usize).saturating_sub(1),
                                _ => return Ok(()),
                            };

                            let mut arg_idx = 2;
                            let (qx, qy, qw, qh) = match args.get(2) {
                                Some(LuaValue::Table(t)) => {
                                    match (
                                        t.get::<f32>("_x"),
                                        t.get::<f32>("_y"),
                                        t.get::<f32>("_w"),
                                        t.get::<f32>("_h"),
                                    ) {
                                        (Ok(x), Ok(y), Ok(w), Ok(h)) => {
                                            arg_idx = 3;
                                            (x, y, w, h)
                                        }
                                        _ => (0.0, 0.0, 0.0, 0.0),
                                    }
                                }
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };

                            let gf = |idx: usize, def: f32| -> f32 {
                                match args.get(idx) {
                                    Some(LuaValue::Number(n)) => *n as f32,
                                    Some(LuaValue::Integer(n)) => *n as f32,
                                    _ => def,
                                }
                            };

                            let entry = crate::state::SpriteBatchEntry {
                                quad_x: qx,
                                quad_y: qy,
                                quad_w: qw,
                                quad_h: qh,
                                x: gf(arg_idx, 0.0),
                                y: gf(arg_idx + 1, 0.0),
                                r: gf(arg_idx + 2, 0.0),
                                sx: gf(arg_idx + 3, 1.0),
                                sy: gf(arg_idx + 4, gf(arg_idx + 3, 1.0)),
                                ox: gf(arg_idx + 5, 0.0),
                                oy: gf(arg_idx + 6, 0.0),
                                color: None,
                            };

                            let mut sbs = sr.sprite_batches.lock();
                            if let Some(data) = sbs.get_mut(&sb_id) {
                                if entry_id < data.entries.len() {
                                    data.entries[entry_id] = entry;
                                }
                            }
                            Ok(())
                        })?,
                    )?;
                }

                // SpriteBatch:clear()
                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "clear",
                        lua.create_function(move |_, self_tbl: LuaTable| {
                            let sb_id = self_tbl.get::<u64>("_spritebatch_id").unwrap_or(0);
                            let mut sbs = sr.sprite_batches.lock();
                            if let Some(data) = sbs.get_mut(&sb_id) {
                                data.entries.clear();
                            }
                            Ok(())
                        })?,
                    )?;
                }

                // SpriteBatch:flush() — noop for software renderer
                sb.set("flush", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;

                // SpriteBatch:getCount()
                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "getCount",
                        lua.create_function(move |_, self_tbl: LuaTable| {
                            let sb_id = self_tbl.get::<u64>("_spritebatch_id").unwrap_or(0);
                            let sbs = sr.sprite_batches.lock();
                            Ok(sbs.get(&sb_id).map(|d| d.entries.len() as i32).unwrap_or(0))
                        })?,
                    )?;
                }

                // SpriteBatch:setColor(r, g, b, a)
                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "setColor",
                        lua.create_function(move |_, args: LuaMultiValue| {
                            let self_tbl = match args.get(0) {
                                Some(LuaValue::Table(t)) => t,
                                _ => return Ok(()),
                            };
                            let sb_id = self_tbl.get::<u64>("_spritebatch_id").unwrap_or(0);
                            let color = if args.len() > 1 {
                                let c = crate::lua_util::parse_color_offset(&args, 1);
                                Some(c)
                            } else {
                                None
                            };
                            let mut sbs = sr.sprite_batches.lock();
                            if let Some(data) = sbs.get_mut(&sb_id) {
                                data.color = color;
                            }
                            Ok(())
                        })?,
                    )?;
                }

                {
                    let sr = Arc::clone(&s);
                    sb.set(
                        "release",
                        lua.create_function(move |_, self_tbl: LuaTable| {
                            if let Ok(id) = self_tbl.get::<u64>("_spritebatch_id") {
                                sr.sprite_batches.lock().remove(&id);
                            }
                            Ok(())
                        })?,
                    )?;
                }
                sb.set(
                    "type",
                    lua.create_function(|_, _self: LuaValue| Ok("SpriteBatch"))?,
                )?;
                sb.set(
                    "typeOf",
                    lua.create_function(|_, (_self, t): (LuaValue, String)| {
                        Ok(t == "SpriteBatch" || t == "Drawable" || t == "Object")
                    })?,
                )?;

                // Attach GC guard
                let guard = lua.create_userdata(ResourceGuard {
                    id: sb_id,
                    kind: ResourceKind::SpriteBatch,
                    state: Arc::clone(&s),
                })?;
                sb.set("_gc_guard", guard)?;

                Ok(LuaValue::Table(sb))
            })?,
        )?;
    }

    Ok(())
}
