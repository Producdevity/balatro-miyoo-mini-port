// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newVideo(path) -> Video
    g.set(
        "newVideo",
        lua.create_function(|lua, _args: LuaMultiValue| {
            let v = lua.create_table()?;
            v.set("play", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            v.set(
                "isPlaying",
                lua.create_function(|_, _self: LuaValue| Ok(false))?,
            )?;
            v.set("pause", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            Ok(LuaValue::Table(v))
        })?,
    )?;

    ui::register(lua, g, state)?;

    // love.graphics.setScissor([x, y, w, h])
    {
        let s = Arc::clone(state);
        g.set(
            "setScissor",
            lua.create_function(move |_, args: LuaMultiValue| {
                if args.len() >= 4 {
                    let x = match args.get(0) {
                        Some(LuaValue::Number(n)) => *n as i32,
                        Some(LuaValue::Integer(n)) => *n as i32,
                        _ => 0,
                    };
                    let y = match args.get(1) {
                        Some(LuaValue::Number(n)) => *n as i32,
                        Some(LuaValue::Integer(n)) => *n as i32,
                        _ => 0,
                    };
                    let w = match args.get(2) {
                        Some(LuaValue::Number(n)) => *n as u32,
                        Some(LuaValue::Integer(n)) => *n as u32,
                        _ => 0,
                    };
                    let h = match args.get(3) {
                        Some(LuaValue::Number(n)) => *n as u32,
                        Some(LuaValue::Integer(n)) => *n as u32,
                        _ => 0,
                    };
                    *s.scissor.lock() = Some((x, y, w, h));
                    s.with_active_buffer(|pb| {
                        pb.scissor = Some((x, y, w, h));
                    });
                } else {
                    *s.scissor.lock() = None;
                    s.with_active_buffer(|pb| {
                        pb.scissor = None;
                    });
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.setShader([shader]) — track active shader for dissolve emulation
    {
        let s = Arc::clone(state);
        g.set(
            "setShader",
            lua.create_function(move |_, args: LuaMultiValue| {
                match args.get(0) {
                    Some(LuaValue::Table(shader_tbl)) => {
                        *s.active_card_shader_inputs.lock() =
                            Some(crate::shader_inputs::shared(shader_tbl)?);
                        *s.active_shader_no_texture.lock() =
                            shader_tbl.get::<bool>("_no_texture").unwrap_or(false);
                        *s.active_card_shader.lock() =
                            shader_tbl.get::<u8>("_card_shader").unwrap_or(0);
                        // Read dissolve/shadow uniforms from shader's _uniforms table
                        if let Ok(uniforms) = shader_tbl.get::<LuaTable>("_uniforms") {
                            let dissolve: f32 = uniforms.get("dissolve").unwrap_or(0.0);
                            let shadow: bool = uniforms.get("shadow").unwrap_or(false);
                            *s.active_shader_dissolve.lock() = dissolve;
                            *s.active_shader_shadow.lock() = shadow;
                        }
                        // Detect fullscreen post-processing shaders (CRT, etc.)
                        if let Ok(uniforms) = shader_tbl.get::<LuaTable>("_uniforms") {
                            let is_fullscreen =
                                uniforms.get::<LuaValue>("scanlines").unwrap_or(LuaNil) != LuaNil
                                    || uniforms.get::<LuaValue>("crt_intensity").unwrap_or(LuaNil)
                                        != LuaNil;
                            *s.active_shader_fullscreen.lock() = is_fullscreen;
                        } else {
                            *s.active_shader_fullscreen.lock() = false;
                        }
                        // Detect background procedural shader
                        *s.active_shader_background.lock() =
                            shader_tbl.get::<bool>("_is_background").unwrap_or(false);
                        // Detect flame shader
                        *s.active_shader_flame.lock() =
                            shader_tbl.get::<bool>("_is_flame").unwrap_or(false);
                        // Detect flash shader
                        *s.active_shader_flash.lock() =
                            shader_tbl.get::<bool>("_is_flash").unwrap_or(false);
                    }
                    _ => {
                        // setShader() with no args = clear shader
                        *s.active_shader_no_texture.lock() = false;
                        *s.active_shader_dissolve.lock() = 0.0;
                        *s.active_shader_shadow.lock() = false;
                        *s.active_shader_fullscreen.lock() = false;
                        *s.active_shader_background.lock() = false;
                        *s.active_card_shader.lock() = 0;
                        *s.active_card_shader_inputs.lock() = None;
                        *s.active_shader_flame.lock() = false;
                        *s.active_shader_flash.lock() = false;
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // Balatro normally sends ten uniforms before each sprite draw. The software
    // renderer consumes only this subset, so the handheld script updates it in
    // one Lua-to-Rust call.
    {
        let s = Arc::clone(state);
        g.set(
            "_setSoftwareShader",
            lua.create_function(
                move |_,
                      (shader, dissolve, shadow, time, burn1, burn2, values): (
                    LuaTable,
                    f32,
                    bool,
                    f32,
                    LuaTable,
                    LuaTable,
                    Option<LuaTable>,
                )| {
                    *s.active_card_shader_inputs.lock() =
                        Some(crate::shader_inputs::update_fast(&shader, time, values)?);
                    *s.active_shader_no_texture.lock() =
                        shader.get::<bool>("_no_texture").unwrap_or(false);
                    *s.active_card_shader.lock() = shader.get::<u8>("_card_shader").unwrap_or(0);
                    *s.active_shader_dissolve.lock() = dissolve;
                    *s.active_shader_shadow.lock() = shadow;
                    *s.active_shader_fullscreen.lock() = false;
                    *s.active_shader_background.lock() =
                        shader.get::<bool>("_is_background").unwrap_or(false);
                    *s.active_shader_flame.lock() =
                        shader.get::<bool>("_is_flame").unwrap_or(false);
                    *s.active_shader_flash.lock() =
                        shader.get::<bool>("_is_flash").unwrap_or(false);
                    s.background_shader_params.lock()[0] = time;
                    *s.dissolve_burn_colour_1.lock() = read_shader_colour(&burn1);
                    *s.dissolve_burn_colour_2.lock() = read_shader_colour(&burn2);
                    Ok(())
                },
            )?,
        )?;
    }

    // love.graphics.setDefaultFilter(min [, mag, anisotropy])
    {
        let s = Arc::clone(&state);
        g.set(
            "setDefaultFilter",
            lua.create_function(move |_, args: LuaMultiValue| {
                let mode = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => "nearest".to_string(),
                };
                *s.default_filter_linear.lock() = mode == "linear";
                Ok(())
            })?,
        )?;
    }

    // Noop stubs for features that don't affect pixel output
    for name in &["setLineStyle", "setLineJoin", "setColorMask"] {
        g.set(
            *name,
            lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
        )?;
    }

    // love.graphics.stencil(func [, action, value, keepvalues])
    // Executes the drawing function with stencil write mode enabled,
    // so shapes drawn inside become the stencil mask.
    {
        let s = Arc::clone(&state);
        g.set(
            "stencil",
            lua.create_function(
                move |_,
                      (func, _action, _value, _keep): (
                    LuaFunction,
                    Option<String>,
                    Option<u8>,
                    Option<bool>,
                )| {
                    let _profile = ProfileTimer::start(&STENCIL_CALLS, &STENCIL_NS);
                    // Clear stencil buffer and enter stencil write mode
                    s.with_active_buffer(|pb| {
                        pb.clear_stencil();
                        pb.stencil_write_mode = true;
                    });
                    s.set_render_queue_allowed(false);

                    // Execute the stencil drawing function — all draw calls
                    // will write to the stencil buffer instead of pixels
                    let result = func.call::<()>(());
                    s.set_render_queue_allowed(true);

                    // Exit stencil write mode
                    s.with_active_buffer(|pb| {
                        pb.stencil_write_mode = false;
                    });

                    if let Err(e) = result {
                        eprintln!("[STENCIL] error in stencil function: {}", e);
                    }
                    Ok(())
                },
            )?,
        )?;
    }

    // love.graphics.setStencilTest([comparemode, comparevalue])
    {
        let s = Arc::clone(&state);
        g.set(
            "setStencilTest",
            lua.create_function(move |_, args: LuaMultiValue| {
                if args.is_empty() {
                    // Disable stencil test
                    *s.stencil_compare.lock() = StencilCompare::Disabled;
                    *s.stencil_ref.lock() = 0;
                } else {
                    let mode_str = match args.get(0) {
                        Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                        _ => "always".to_string(),
                    };
                    let ref_val = match args.get(1) {
                        Some(LuaValue::Integer(n)) => *n as u8,
                        Some(LuaValue::Number(n)) => *n as u8,
                        _ => 0,
                    };
                    let compare = match mode_str.as_str() {
                        "greater" => StencilCompare::Greater,
                        "gequal" => StencilCompare::GEqual,
                        "equal" => StencilCompare::Equal,
                        "lequal" => StencilCompare::LEqual,
                        "less" => StencilCompare::Less,
                        "notequal" => StencilCompare::NotEqual,
                        "always" => StencilCompare::Always,
                        "never" => StencilCompare::Never,
                        _ => StencilCompare::Disabled,
                    };
                    *s.stencil_compare.lock() = compare;
                    *s.stencil_ref.lock() = ref_val;
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.setBlendMode(mode [, alphamode])
    {
        let s = Arc::clone(&state);
        g.set(
            "setBlendMode",
            lua.create_function(move |_, args: LuaMultiValue| {
                let mode_str = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => "alpha".to_string(),
                };
                let mode = match mode_str.as_str() {
                    "replace" => BlendMode::Replace,
                    "multiply" | "multiplicative" => BlendMode::Multiply,
                    "add" | "additive" => BlendMode::Add,
                    "screen" => BlendMode::Screen,
                    _ => BlendMode::Alpha,
                };
                *s.blend_mode.lock() = mode;
                Ok(())
            })?,
        )?;
    }

    // love.graphics.applyTransform(transform) — apply a Transform object
    {
        let s = Arc::clone(state);
        g.set(
            "applyTransform",
            lua.create_function(move |_, t_obj: LuaTable| {
                // Read 6 transform coefficients from the Transform userdata/table
                let a: f32 = t_obj.get("a").unwrap_or(1.0);
                let b: f32 = t_obj.get("b").unwrap_or(0.0);
                let c: f32 = t_obj.get("c").unwrap_or(0.0);
                let d: f32 = t_obj.get("d").unwrap_or(1.0);
                let tx: f32 = t_obj.get("tx").unwrap_or(0.0);
                let ty: f32 = t_obj.get("ty").unwrap_or(0.0);

                let mut stack = s.transform_stack.lock();
                if let Some(current) = stack.last_mut() {
                    // Multiply current transform by the applied transform
                    let na = current.a * a + current.b * c;
                    let nb = current.a * b + current.b * d;
                    let nc = current.c * a + current.d * c;
                    let nd = current.c * b + current.d * d;
                    let ntx = current.a * tx + current.b * ty + current.tx;
                    let nty = current.c * tx + current.d * ty + current.ty;
                    current.a = na;
                    current.b = nb;
                    current.c = nc;
                    current.d = nd;
                    current.tx = ntx;
                    current.ty = nty;
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.replaceTransform(transform) — replace current transform
    {
        let s = Arc::clone(state);
        g.set(
            "replaceTransform",
            lua.create_function(move |_, t_obj: LuaTable| {
                let a: f32 = t_obj.get("a").unwrap_or(1.0);
                let b: f32 = t_obj.get("b").unwrap_or(0.0);
                let c: f32 = t_obj.get("c").unwrap_or(0.0);
                let d: f32 = t_obj.get("d").unwrap_or(1.0);
                let tx: f32 = t_obj.get("tx").unwrap_or(0.0);
                let ty: f32 = t_obj.get("ty").unwrap_or(0.0);

                let mut stack = s.transform_stack.lock();
                if let Some(current) = stack.last_mut() {
                    current.a = a;
                    current.b = b;
                    current.c = c;
                    current.d = d;
                    current.tx = tx;
                    current.ty = ty;
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.transformPoint(x, y) — apply current transform to point
    {
        let s = Arc::clone(state);
        g.set(
            "transformPoint",
            lua.create_function(move |_, (x, y): (f32, f32)| {
                let t = current_transform(&s);
                let (px, py) = t.apply(x, y);
                Ok((px as f64, py as f64))
            })?,
        )?;
    }

    // love.graphics.inverseTransformPoint(x, y) — apply inverse transform to point
    {
        let s = Arc::clone(state);
        g.set(
            "inverseTransformPoint",
            lua.create_function(move |_, (x, y): (f32, f32)| {
                let t = current_transform(&s);
                if let Some(inv) = t.inverse() {
                    let (px, py) = inv.apply(x, y);
                    Ok((px as f64, py as f64))
                } else {
                    Ok((x as f64, y as f64))
                }
            })?,
        )?;
    }

    // love.graphics.getBackgroundColor()
    {
        let s = Arc::clone(state);
        g.set(
            "getBackgroundColor",
            lua.create_function(move |_, ()| {
                let c = *s.background_color.lock();
                Ok((c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64))
            })?,
        )?;
    }

    // love.graphics.getBlendMode()
    {
        let s = Arc::clone(state);
        g.set(
            "getBlendMode",
            lua.create_function(move |_, ()| {
                let mode_str = match *s.blend_mode.lock() {
                    BlendMode::Alpha => "alpha",
                    BlendMode::Replace => "replace",
                    BlendMode::Multiply => "multiply",
                    BlendMode::Add => "add",
                    BlendMode::Screen => "screen",
                };
                Ok((mode_str, "alphamultiply"))
            })?,
        )?;
    }

    // love.graphics.getColorMask()
    g.set(
        "getColorMask",
        lua.create_function(|_, ()| Ok((true, true, true, true)))?,
    )?;

    // love.graphics.getShader()
    g.set("getShader", lua.create_function(|_, ()| Ok(LuaNil))?)?;

    // love.graphics.getDPIScale()
    g.set("getDPIScale", lua.create_function(|_, ()| Ok(1.0f64))?)?;

    // love.graphics.getScissor()
    {
        let s = Arc::clone(state);
        g.set(
            "getScissor",
            lua.create_function(move |_, ()| match *s.scissor.lock() {
                Some((x, y, w, h)) => Ok((
                    LuaValue::Integer(x as _),
                    LuaValue::Integer(y as _),
                    LuaValue::Integer(w as _),
                    LuaValue::Integer(h as _),
                )),
                None => Ok((LuaNil, LuaNil, LuaNil, LuaNil)),
            })?,
        )?;
    }

    // love.graphics.getLineStyle()
    g.set("getLineStyle", lua.create_function(|_, ()| Ok("smooth"))?)?;

    // love.graphics.getLineJoin()
    g.set("getLineJoin", lua.create_function(|_, ()| Ok("miter"))?)?;

    // love.graphics.getRendererInfo()
    g.set(
        "getRendererInfo",
        lua.create_function(|_, ()| Ok(("love-terminal", "0.1", "Software", "Terminal")))?,
    )?;

    // love.graphics.getSupported()
    g.set(
        "getSupported",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set("canvas", true)?;
            t.set("multicanvas", false)?;
            t.set("shader", true)?;
            Ok(t)
        })?,
    )?;

    // love.graphics.captureScreenshot(filename)
    g.set(
        "captureScreenshot",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;

    // love.graphics.newImageData — some games create ImageData via graphics module
    {
        let s = Arc::clone(state);
        g.set(
            "newImageData",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let w = match args.get(0) {
                    Some(LuaValue::Number(n)) => *n as u32,
                    Some(LuaValue::Integer(n)) => *n as u32,
                    _ => 1,
                };
                let h = match args.get(1) {
                    Some(LuaValue::Number(n)) => *n as u32,
                    Some(LuaValue::Integer(n)) => *n as u32,
                    _ => 1,
                };
                create_image_data_table(lua, &s, w, h)
            })?,
        )?;
    }

    // love.graphics.getDefaultFilter()
    {
        let s = Arc::clone(&state);
        g.set(
            "getDefaultFilter",
            lua.create_function(move |_, ()| {
                let mode = if *s.default_filter_linear.lock() {
                    "linear"
                } else {
                    "nearest"
                };
                Ok((mode, mode, 1i32))
            })?,
        )?;
    }

    // love.graphics.getStencilTest()
    g.set(
        "getStencilTest",
        lua.create_function(|_, ()| Ok((false, "always")))?,
    )?;

    Ok(())
}
