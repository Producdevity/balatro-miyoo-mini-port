// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newShader(code) -> Shader
    {
        let s = Arc::clone(state);
        g.set(
            "newShader",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let shader_tbl = lua.create_table()?;
                crate::shader_inputs::register(lua, &shader_tbl)?;
                // Store a uniforms table to track sent values
                let uniforms = lua.create_table()?;
                shader_tbl.set("_uniforms", uniforms)?;

                shader_tbl.set("_is_background", false)?;

                // Detect shaders that don't sample the texture (procedural output only).
                // Drawing raw texture pixels for these shaders is always wrong.
                let source_str = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => String::new(),
                };
                let card_shader = card_shader_for_source(&source_str);
                shader_tbl.set("_card_shader", card_shader)?;

                let is_flame = source_str.contains("flame");
                let is_flash = source_str.ends_with("/flash.fs")
                    || (source_str.contains("mid_white") && source_str.contains("mid_flash"));
                let no_texture = source_str.contains("background")
                    || source_str.contains("splash")
                    || source_str.contains("skew")
                    || source_str.contains("vortex")
                    || is_flame
                    || is_flash;
                shader_tbl.set("_no_texture", no_texture)?;
                shader_tbl.set("_is_flame", is_flame)?;
                shader_tbl.set("_is_flash", is_flash)?;

                let sr = Arc::clone(&s);
                shader_tbl.set(
                    "send",
                    lua.create_function(
                        move |_, (self_tbl, name, value): (LuaTable, String, LuaMultiValue)| {
                            if let Some(value) = value.front() {
                                crate::shader_inputs::send(&self_tbl, &name, value)?;
                            }
                            // Store the value in _uniforms table for potential future use
                            if let Ok(u) = self_tbl.get::<LuaTable>("_uniforms") {
                                if let Some(val) = value.get(0) {
                                    u.set(name.as_str(), val.clone()).ok();
                                }
                            }
                            // Auto-detect background shader by uniform names and capture colours.
                            // Only mark the shader table; setShader() reads _is_background.
                            match name.as_str() {
                                "colour_1" | "colour_2" | "colour_3" => {
                                    let is_flame =
                                        self_tbl.get::<bool>("_is_flame").unwrap_or(false);
                                    if is_flame {
                                        // Flame shader: capture colors into flame params
                                        if let Some(LuaValue::Table(ref ct)) = value.get(0) {
                                            let r: f32 = ct.get(1).unwrap_or(0.0);
                                            let g: f32 = ct.get(2).unwrap_or(0.0);
                                            let b: f32 = ct.get(3).unwrap_or(0.0);
                                            let mut fp = sr.flame_shader_params.lock();
                                            match name.as_str() {
                                                "colour_1" => {
                                                    fp[1] = r;
                                                    fp[2] = g;
                                                    fp[3] = b;
                                                }
                                                "colour_2" => {
                                                    fp[4] = r;
                                                    fp[5] = g;
                                                    fp[6] = b;
                                                }
                                                _ => {}
                                            }
                                        }
                                    } else {
                                        self_tbl.set("_is_background", true).ok();
                                        if let Some(LuaValue::Table(ref ct)) = value.get(0) {
                                            let r: f32 = ct.get(1).unwrap_or(0.0);
                                            let g: f32 = ct.get(2).unwrap_or(0.0);
                                            let b: f32 = ct.get(3).unwrap_or(0.0);
                                            let a: f32 = ct.get(4).unwrap_or(1.0);
                                            let mut colours = sr.background_shader_colours.lock();
                                            match name.as_str() {
                                                "colour_1" => colours[0] = [r, g, b, a],
                                                "colour_2" => colours[1] = [r, g, b, a],
                                                "colour_3" => colours[2] = [r, g, b, a],
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                "amount" => {
                                    // Flame shader intensity
                                    if let Some(val) = value.get(0) {
                                        let v = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        sr.flame_shader_params.lock()[0] = v;
                                    }
                                }
                                "id" => {
                                    // Flame shader: per-instance id for variation
                                    if let Some(val) = value.get(0) {
                                        let v = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        sr.flame_shader_params.lock()[7] = v;
                                    }
                                }
                                "mid_flash" => {
                                    // Flash shader alpha
                                    if let Some(val) = value.get(0) {
                                        let v = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        *sr.flash_shader_alpha.lock() = v;
                                    }
                                }
                                // Live-update dissolve/shadow so mid-frame sends take effect
                                "dissolve" => {
                                    if let Some(val) = value.get(0) {
                                        let d = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        *sr.active_shader_dissolve.lock() = d;
                                    }
                                }
                                "shadow" => {
                                    if let Some(val) = value.get(0) {
                                        let b = match val {
                                            LuaValue::Boolean(b) => *b,
                                            _ => false,
                                        };
                                        *sr.active_shader_shadow.lock() = b;
                                    }
                                }
                                "burn_colour_1" | "burn_colour_2" => {
                                    if let Some(LuaValue::Table(ref ct)) = value.get(0) {
                                        let r: f32 = ct.get(1).unwrap_or(0.0);
                                        let g: f32 = ct.get(2).unwrap_or(0.0);
                                        let b: f32 = ct.get(3).unwrap_or(0.0);
                                        let a: f32 = ct.get(4).unwrap_or(1.0);
                                        let target = if name == "burn_colour_1" {
                                            &sr.dissolve_burn_colour_1
                                        } else {
                                            &sr.dissolve_burn_colour_2
                                        };
                                        *target.lock() = [r, g, b, a];
                                    }
                                }
                                "time" | "spin_time" | "spin_amount" | "contrast" => {
                                    if let Some(val) = value.get(0) {
                                        let v = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        if name == "time" {
                                            // Flame shader has its own custom timer
                                            let is_flame =
                                                self_tbl.get::<bool>("_is_flame").unwrap_or(false);
                                            if is_flame {
                                                sr.flame_shader_params.lock()[8] = v;
                                            }
                                            if self_tbl.get::<bool>("_is_flash").unwrap_or(false) {
                                                *sr.flash_shader_time.lock() = v;
                                            }
                                        }
                                        let mut params = sr.background_shader_params.lock();
                                        match name.as_str() {
                                            "time" => params[0] = v,
                                            "spin_time" => params[1] = v,
                                            "spin_amount" => params[2] = v,
                                            "contrast" => params[3] = v,
                                            _ => {}
                                        }
                                    }
                                }
                                "bloom_fac" | "crt_intensity" => {
                                    if let Some(val) = value.get(0) {
                                        let v = match val {
                                            LuaValue::Number(n) => *n as f32,
                                            LuaValue::Integer(n) => *n as f32,
                                            _ => 0.0,
                                        };
                                        let mut params = sr.crt_params.lock();
                                        match name.as_str() {
                                            "bloom_fac" => params[0] = v,
                                            "crt_intensity" => params[1] = v,
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                            Ok(())
                        },
                    )?,
                )?;
                shader_tbl.set(
                    "hasUniform",
                    lua.create_function(|_, (_self, _name): (LuaValue, String)| Ok(true))?,
                )?;
                shader_tbl.set("release", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
                shader_tbl.set(
                    "type",
                    lua.create_function(|_, _self: LuaValue| Ok("Shader"))?,
                )?;
                shader_tbl.set(
                    "typeOf",
                    lua.create_function(|_, (_self, t): (LuaValue, String)| {
                        Ok(t == "Shader" || t == "Object")
                    })?,
                )?;
                Ok(LuaValue::Table(shader_tbl))
            })?,
        )?;
    }

    Ok(())
}
