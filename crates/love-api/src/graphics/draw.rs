// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn register(
    lua: &Lua,
    g: &LuaTable,
    state: &Arc<SharedState>,
    queued_output: bool,
) -> LuaResult<()> {
    // love.graphics.draw(drawable, [quad], x, y, r, sx, sy, ox, oy)
    {
        let s = Arc::clone(&state);
        g.set(
            "draw",
            lua.create_function(move |_, args: LuaMultiValue| {
                let _profile = ProfileTimer::start(&DRAW_CALLS, &DRAW_NS);
                if args.is_empty() {
                    return Ok(());
                }

                let drawable = match args.get(0) {
                    Some(LuaValue::Table(t)) => t.clone(),
                    _ => return Ok(()),
                };

                // Loaded images carry a native handle so the hot sprite path does
                // not have to look the image up in the registry on every draw.
                let image_handle: Option<LuaAnyUserData> = drawable.get("_native_image").ok();
                let image_id: Option<u64> = if let Some(handle) = image_handle.as_ref() {
                    Some(handle.borrow::<ImageHandle>()?.id)
                } else {
                    drawable.get("_image_id").ok()
                };
                let canvas_id: Option<u64> = if image_id.is_none() {
                    drawable.get("_canvas_id").ok()
                } else {
                    None
                };
                let spritebatch_id: Option<u64> = if image_id.is_none() && canvas_id.is_none() {
                    drawable.get("_spritebatch_id").ok()
                } else {
                    None
                };

                if image_id.is_none() && canvas_id.is_none() && spritebatch_id.is_none() {
                    return Ok(());
                }

                // Parse remaining args: could be (quad, x, y, ...) or (x, y, ...)
                let mut arg_idx = 1;
                let (quad_x, quad_y, quad_w, quad_h, has_quad) = match args.get(1) {
                    Some(LuaValue::Table(t)) => {
                        if let Ok(handle) = t.get::<LuaAnyUserData>("_native_quad") {
                            let quad = handle.borrow::<QuadData>()?;
                            arg_idx = 2;
                            (quad.x, quad.y, quad.w, quad.h, true)
                        } else {
                            // Keep compatibility with Quad-like tables created by game code.
                            match (
                                t.get::<f32>("_x"),
                                t.get::<f32>("_y"),
                                t.get::<f32>("_w"),
                                t.get::<f32>("_h"),
                            ) {
                                (Ok(qx), Ok(qy), Ok(qw), Ok(qh)) => {
                                    arg_idx = 2;
                                    (qx, qy, qw, qh, true)
                                }
                                _ => (0.0, 0.0, 0.0, 0.0, false),
                            }
                        }
                    }
                    _ => (0.0, 0.0, 0.0, 0.0, false),
                };

                let get_f32 = |idx: usize| -> f32 {
                    match args.get(idx) {
                        Some(LuaValue::Number(n)) => *n as f32,
                        Some(LuaValue::Integer(n)) => *n as f32,
                        _ => 0.0,
                    }
                };

                let x = get_f32(arg_idx);
                let y = get_f32(arg_idx + 1);
                let r = get_f32(arg_idx + 2);
                let sx = match args.get(arg_idx + 3) {
                    Some(LuaValue::Number(n)) => *n as f32,
                    Some(LuaValue::Integer(n)) => *n as f32,
                    _ => 1.0,
                };
                let sy = match args.get(arg_idx + 4) {
                    Some(LuaValue::Number(n)) => *n as f32,
                    Some(LuaValue::Integer(n)) => *n as f32,
                    _ => sx,
                };
                let ox = get_f32(arg_idx + 5);
                let oy = get_f32(arg_idx + 6);

                let t = current_transform(&s);
                let mut color = color_f32_to_u8(*s.current_color.lock());
                let replace = *s.blend_mode.lock() == BlendMode::Replace;
                let is_fullscreen_shader = *s.active_shader_fullscreen.lock();

                // Dissolve shader emulation — build DissolveParams for per-pixel noise
                let dissolve = *s.active_shader_dissolve.lock();
                let is_shadow = *s.active_shader_shadow.lock();

                if dissolve > 0.6 {
                    // Fully dissolved — skip draw entirely
                    return Ok(());
                }

                if is_shadow {
                    // Shadow pass: skip entirely in TUI mode.
                    // Terminal's 2-color-per-cell quantizer makes semi-transparent
                    // shadows visible as "ghost copies" of every element.
                    return Ok(());
                }

                // Card shader effects
                let card_shader = *s.active_card_shader.lock();
                // Spatial shaders: per-pixel effects applied during blit
                let has_spatial_shader = card_shader >= 1 && card_shader <= 11;
                let shader_inputs = s
                    .active_card_shader_inputs
                    .lock()
                    .as_ref()
                    .map(|inputs| *inputs.lock())
                    .unwrap_or_default();

                let dp = if (dissolve > 0.01 && !is_shadow) || (!is_shadow && has_spatial_shader) {
                    let (b1_arr, b2_arr) = if dissolve > 0.01 {
                        let b1 = *s.dissolve_burn_colour_1.lock();
                        let b2 = *s.dissolve_burn_colour_2.lock();
                        (
                            [
                                (b1[0] * 255.0) as u8,
                                (b1[1] * 255.0) as u8,
                                (b1[2] * 255.0) as u8,
                                (b1[3] * 255.0) as u8,
                            ],
                            [
                                (b2[0] * 255.0) as u8,
                                (b2[1] * 255.0) as u8,
                                (b2[2] * 255.0) as u8,
                                (b2[3] * 255.0) as u8,
                            ],
                        )
                    } else {
                        ([0, 0, 0, 0], [0, 0, 0, 0])
                    };
                    DissolveParams {
                        dissolve,
                        burn1: b1_arr,
                        burn2: b2_arr,
                        shader_effect: card_shader,
                        shader_inputs,
                        sprite_w: if has_quad { quad_w } else { 71.0 },
                        sprite_h: if has_quad { quad_h } else { 95.0 },
                    }
                } else {
                    DissolveParams::NONE
                };

                // Uniform card shader effects (non-spatial: hologram only)
                if !is_shadow && card_shader == 9 {
                    // Hologram: translucent cyan-blue ghost effect
                    let avg = ((color[0] as u16 + color[1] as u16 + color[2] as u16) / 3) as u8;
                    color[0] = (avg as u16 * 77 / 255).min(255) as u8;
                    color[1] = (avg as u16 * 200 / 255).min(255) as u8;
                    color[2] = (avg as u16 * 240 / 255).min(255) as u8;
                    color[3] = (color[3] as u16 * 180 / 255) as u8;
                }

                if let Some(canvas_id) = canvas_id {
                    s.flush_render_jobs();
                    // When a fullscreen post-processing shader is active (CRT),
                    // use replace mode since the shader would fully overwrite the target
                    let use_replace = replace || is_fullscreen_shader;

                    // Canvas pixels are premultiplied alpha in LÖVE2D.
                    // Use premultiplied blend (mode 4) unless replace is requested.
                    let canvas_blend: u8 = if use_replace { 1 } else { 4 };

                    // Optimize: avoid cloning canvas pixels when drawing to screen
                    let active = *s.active_canvas.lock();
                    if active == 0 {
                        // Canvas → screen: hold both locks, no clone needed
                        let canvases = s.canvases.lock();
                        if let Some(cb) = canvases.get(&canvas_id) {
                            let identity_transform = (t.a - 1.0).abs() < f32::EPSILON
                                && t.b.abs() < f32::EPSILON
                                && t.c.abs() < f32::EPSILON
                                && (t.d - 1.0).abs() < f32::EPSILON
                                && t.tx.abs() < f32::EPSILON
                                && t.ty.abs() < f32::EPSILON;
                            if !has_quad
                                && !use_replace
                                && identity_transform
                                && x == 0.0
                                && y == 0.0
                                && r == 0.0
                                && sx == 1.0
                                && sy == 1.0
                                && ox == 0.0
                                && oy == 0.0
                                && color == [255, 255, 255, 255]
                            {
                                let mut pb = s.pixel_buffer.lock();
                                pb.scissor = *s.scissor.lock();
                                pb.stencil_compare = *s.stencil_compare.lock();
                                if !pb.stencil_write_mode
                                    && pb.stencil_compare == StencilCompare::Disabled
                                {
                                    pb.composite_premultiplied_identity(
                                        &cb.pixels, cb.width, cb.height, 0, 0,
                                    );
                                    return Ok(());
                                }
                            }
                            let (src_x, src_y, src_w, src_h) = if has_quad {
                                (quad_x, quad_y, quad_w, quad_h)
                            } else {
                                (0.0, 0.0, cb.width as f32, cb.height as f32)
                            };
                            let mut pb = s.pixel_buffer.lock();
                            pb.blend = canvas_blend;
                            pb.scissor = *s.scissor.lock();
                            pb.stencil_compare = *s.stencil_compare.lock();
                            pb.stencil_ref = *s.stencil_ref.lock();
                            draw_region_to_buf(
                                &mut pb,
                                &cb.pixels,
                                cb.width,
                                cb.height,
                                src_x,
                                src_y,
                                src_w,
                                src_h,
                                x,
                                y,
                                r,
                                sx,
                                sy,
                                ox,
                                oy,
                                &t,
                                color,
                                use_replace,
                                false,
                                dp,
                                None,
                            );
                        }
                    } else {
                        // Canvas → canvas: clone to avoid potential self-reference
                        let canvas_snapshot = {
                            let canvases = s.canvases.lock();
                            canvases
                                .get(&canvas_id)
                                .map(|cb| (cb.width, cb.height, cb.pixels.clone()))
                        };
                        if let Some((cw, ch, ref pixels)) = canvas_snapshot {
                            let (src_x, src_y, src_w, src_h) = if has_quad {
                                (quad_x, quad_y, quad_w, quad_h)
                            } else {
                                (0.0, 0.0, cw as f32, ch as f32)
                            };
                            // Draw in single with_active_buffer call to keep canvas_blend
                            // (with_active_buffer resets blend from global state)
                            s.with_active_buffer(|pb| {
                                pb.blend = canvas_blend;
                                draw_region_to_buf(
                                    pb,
                                    pixels,
                                    cw,
                                    ch,
                                    src_x,
                                    src_y,
                                    src_w,
                                    src_h,
                                    x,
                                    y,
                                    r,
                                    sx,
                                    sy,
                                    ox,
                                    oy,
                                    &t,
                                    color,
                                    use_replace,
                                    false,
                                    dp,
                                    None,
                                );
                            });
                        }
                    }
                } else if let Some(image_id) = image_id {
                    let image = if let Some(handle) = image_handle.as_ref() {
                        Arc::clone(&handle.borrow::<ImageHandle>()?.data)
                    } else {
                        let images = s.images.lock();
                        let Some(image) = images.get(&image_id) else {
                            return Ok(());
                        };
                        Arc::clone(image)
                    };
                    // Skip draws when a no-texture shader is active (procedural output only).
                    // Background, flame, and flash shaders are handled specially.
                    let is_bg_shader = *s.active_shader_background.lock();
                    let is_no_tex = *s.active_shader_no_texture.lock();
                    let is_flame = *s.active_shader_flame.lock();
                    let is_flash = *s.active_shader_flash.lock();
                    if !is_bg_shader && !is_flame && !is_flash && is_no_tex {
                        return Ok(());
                    }
                    if is_flash {
                        // Flash shader: white overlay with mid_flash alpha
                        let mid_flash = *s.flash_shader_alpha.lock();
                        if mid_flash > 0.01 {
                            let (scale_x, scale_y) = t.scale_factor();
                            let (src_w, src_h) = if has_quad {
                                (quad_w, quad_h)
                            } else {
                                (image.width as f32, image.height as f32)
                            };
                            let dst_w = (src_w * sx.abs() * scale_x) as i32;
                            let dst_h = (src_h * sy.abs() * scale_y) as i32;
                            let (tx, ty) = t.apply(x - ox * sx, y - oy * sy);
                            let time = *s.flash_shader_time.lock();
                            s.with_active_buffer(|pb| {
                                sprite_to_text::flash::draw(
                                    pb,
                                    [tx as i32, ty as i32, tx as i32 + dst_w, ty as i32 + dst_h],
                                    time,
                                    mid_flash,
                                );
                            });
                        }
                        return Ok(());
                    }
                    if is_flame {
                        // Flame shader: per-pixel turbulent fire effect matching flame.fs GLSL
                        let fp = *s.flame_shader_params.lock();
                        let amount = fp[0];
                        let intensity = amount.min(10.0);
                        if intensity > 0.1 {
                            let (scale_x, scale_y) = t.scale_factor();
                            let (src_w, src_h) = if has_quad {
                                (quad_w, quad_h)
                            } else {
                                (image.width as f32, image.height as f32)
                            };
                            let dst_w = (src_w * sx.abs() * scale_x) as i32;
                            let dst_h = (src_h * sy.abs() * scale_y) as i32;
                            let (tx, ty) = t.apply(x - ox * sx, y - oy * sy);
                            let c1 = [fp[1], fp[2], fp[3]];
                            let c2 = [fp[4], fp[5], fp[6]];
                            let flame_id = fp[7];
                            let time = fp[8]; // custom timer from Lua (advances at variable speed)
                            let cache = std::env::var("BALATRO_FLAME_CACHE").as_deref() != Ok("0");
                            let rect = [tx as i32, ty as i32, dst_w, dst_h];
                            let draw = move |pb: &mut PixelBuffer| {
                                let mut flame = sprite_to_text::flame::Flame::new(
                                    time, intensity, flame_id, c1, c2, cache,
                                );
                                flame.draw(pb, rect);
                            };
                            static DEFER_FLAME: LazyLock<bool> = LazyLock::new(|| {
                                std::env::var("BALATRO_DEFER_FLAME").as_deref() != Ok("0")
                            });
                            if !*DEFER_FLAME || !defer_screen_draw(&s, draw) {
                                s.with_active_buffer(draw);
                            }
                        }
                        return Ok(());
                    }
                    if is_bg_shader {
                        let colours = *s.background_shader_colours.lock();
                        let bg_params = *s.background_shader_params.lock();
                        let (scale_x, scale_y) = t.scale_factor();
                        let (src_w, src_h) = if has_quad {
                            (quad_w, quad_h)
                        } else {
                            (image.width as f32, image.height as f32)
                        };
                        let dst_w = (src_w * sx.abs() * scale_x) as i32;
                        let dst_h = (src_h * sy.abs() * scale_y) as i32;
                        let (tx, ty) = t.apply(x - ox * sx, y - oy * sy);
                        let cache = Arc::clone(&s.background_cache);
                        let cached_background = std::env::var_os("BALATRO_PLATFORM").as_deref()
                            == Some(std::ffi::OsStr::new("miyoo"));
                        let draw = move |pb: &mut PixelBuffer| {
                            if cached_background {
                                let mut cache = cache.lock();
                                if cache.needs_refresh(pb.width, pb.height, &colours) {
                                    if cache.buffer.width != pb.width
                                        || cache.buffer.height != pb.height
                                    {
                                        cache.buffer.resize(pb.width, pb.height);
                                    }
                                    cache.buffer.fill_procedural_background(
                                        tx as i32, ty as i32, dst_w, dst_h, &colours, bg_params,
                                    );
                                    cache.colours = colours;
                                    cache.valid = true;
                                }
                                pb.pixels.copy_from_slice(&cache.buffer.pixels);
                            } else {
                                pb.fill_procedural_background(
                                    tx as i32, ty as i32, dst_w, dst_h, &colours, bg_params,
                                );
                            }
                        };
                        if queued_output && defer_screen_draw(&s, draw.clone()) {
                            return Ok(());
                        }
                        s.with_active_buffer(draw);
                    } else {
                        let (src_x, src_y, src_w, src_h) = if has_quad {
                            (quad_x, quad_y, quad_w, quad_h)
                        } else {
                            (0.0, 0.0, image.width as f32, image.height as f32)
                        };
                        let deferred_image = Arc::clone(&image);
                        let deferred_transform = t.clone();
                        let prepared = if crate::card_cache::eligible(
                            dp,
                            color,
                            *s.default_filter_linear.lock(),
                        ) {
                            s.card_cache.lock().get(
                                &image,
                                [src_x, src_y, src_w, src_h],
                                dp.shader_effect,
                                dp.shader_inputs.clock != 0.0,
                            )
                        } else {
                            None
                        };
                        let coverage = if crate::occlusion::enabled() && s.can_defer_render() {
                            let mut composite = t.clone();
                            composite.translate(x, y);
                            if r != 0.0 {
                                composite.rotate(r);
                            }
                            composite.scale(sx, sy);
                            composite.translate(-ox, -oy);
                            let clip = (*s.scissor.lock())
                                .map(|(x, y, w, h)| {
                                    crate::occlusion::Rect([
                                        x,
                                        y,
                                        x.saturating_add(w as i32),
                                        y.saturating_add(h as i32),
                                    ])
                                })
                                .unwrap_or(crate::occlusion::Rect([-16384, -16384, 16384, 16384]));
                            s.opacity_cache.lock().coverage(
                                &image,
                                [src_x, src_y, src_w, src_h],
                                &composite,
                                dp,
                                color[3],
                                s.blend_code(),
                                *s.default_filter_linear.lock(),
                                clip,
                            )
                        } else {
                            crate::occlusion::Coverage::default()
                        };
                        #[cfg(feature = "layer-pairs")]
                        if ImageDraw::defer(
                            &s,
                            coverage,
                            &image,
                            [src_x, src_y, src_w, src_h],
                            [x, y, r, sx, sy, ox, oy],
                            &t,
                            color,
                            replace,
                            dp,
                            prepared.clone(),
                        ) {
                            return Ok(());
                        }
                        if defer_covered_draw(&s, coverage, move |buffer| {
                            draw_region_to_buf(
                                buffer,
                                &deferred_image.pixels,
                                deferred_image.width,
                                deferred_image.height,
                                src_x,
                                src_y,
                                src_w,
                                src_h,
                                x,
                                y,
                                r,
                                sx,
                                sy,
                                ox,
                                oy,
                                &deferred_transform,
                                color,
                                replace,
                                false,
                                dp,
                                prepared.as_deref(),
                            );
                        }) {
                            return Ok(());
                        }
                        draw_region(
                            &s,
                            &image.pixels,
                            image.width,
                            image.height,
                            src_x,
                            src_y,
                            src_w,
                            src_h,
                            x,
                            y,
                            r,
                            sx,
                            sy,
                            ox,
                            oy,
                            &t,
                            color,
                            replace,
                            false,
                            dp,
                        );
                    }
                } else if let Some(sb_id) = spritebatch_id {
                    // Skip SpriteBatch draws with no-texture or background shaders
                    if *s.active_shader_no_texture.lock() || *s.active_shader_background.lock() {
                        return Ok(());
                    }
                    // SpriteBatch: hold locks, avoid cloning entries
                    let sbs = s.sprite_batches.lock();
                    if let Some(data) = sbs.get(&sb_id) {
                        let images = s.images.lock();
                        if let Some(img) = images.get(&data.image_id) {
                            let mut batch_t = t.clone();
                            batch_t.translate(x, y);
                            if r.abs() > 0.001 {
                                batch_t.rotate(r);
                            }
                            batch_t.scale(sx, sy);
                            batch_t.translate(-ox, -oy);

                            let batch_tint = match data.color {
                                Some(c) => color_f32_to_u8(c),
                                None => color,
                            };

                            for entry in &data.entries {
                                let (src_x, src_y, src_w, src_h) = if entry.quad_w > 0.0 {
                                    (entry.quad_x, entry.quad_y, entry.quad_w, entry.quad_h)
                                } else {
                                    (0.0, 0.0, img.width as f32, img.height as f32)
                                };
                                draw_region(
                                    &s,
                                    &img.pixels,
                                    img.width,
                                    img.height,
                                    src_x,
                                    src_y,
                                    src_w,
                                    src_h,
                                    entry.x,
                                    entry.y,
                                    entry.r,
                                    entry.sx,
                                    entry.sy,
                                    entry.ox,
                                    entry.oy,
                                    &batch_t,
                                    batch_tint,
                                    replace,
                                    false,
                                    dp,
                                );
                            }
                        }
                    }
                }

                Ok(())
            })?,
        )?;
    }

    Ok(())
}

pub(super) fn current_transform(state: &SharedState) -> Transform {
    state
        .transform_stack
        .lock()
        .last()
        .cloned()
        .unwrap_or_default()
}

pub(super) fn prepare_draw_transform(
    mut transform: Transform,
    world_scale: f32,
    x: f32,
    y: f32,
    angle: f32,
    origin_x: f32,
    origin_y: f32,
    object_scale: f32,
) -> Transform {
    transform.scale(world_scale, world_scale);
    transform.translate(x, y);
    if angle != 0.0 {
        transform.rotate(angle);
    }
    transform.translate(origin_x, origin_y);
    transform.scale(object_scale, object_scale);
    transform
}

/// Draw an image region without acquiring the active-buffer lock.
pub(super) fn draw_region_to_buf(
    pb: &mut PixelBuffer,
    src_pixels: &[u8],
    src_w: u32,
    src_h: u32,
    src_x: f32,
    src_y: f32,
    src_rw: f32,
    src_rh: f32,
    x: f32,
    y: f32,
    r: f32,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    t: &Transform,
    color: [u8; 4],
    replace: bool,
    source_white_mask: bool,
    dp: DissolveParams,
    prepared: Option<&sprite_to_text::prepared_card::PreparedCard>,
) {
    let mut composite = t.clone();
    composite.translate(x, y);
    if r != 0.0 {
        composite.rotate(r);
    }
    composite.scale(sx, sy);
    composite.translate(-ox, -oy);

    // The fast rasterizer requires positive, axis-aligned scales. Rotation can
    // come from the transform stack, not just the draw call's angle argument.
    if composite.b != 0.0 || composite.c != 0.0 || composite.a <= 0.0 || composite.d <= 0.0 {
        let inv = match composite.inverse() {
            Some(inv) => inv,
            None => return,
        };
        let corners = [
            composite.apply(0.0, 0.0),
            composite.apply(src_rw, 0.0),
            composite.apply(src_rw, src_rh),
            composite.apply(0.0, src_rh),
        ];
        let min_x = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor() as i32;
        let max_x = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil() as i32;
        let min_y = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor() as i32;
        let max_y = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil() as i32;
        let started = PROFILE_ENABLED.then(Instant::now);
        if let Some(prepared) = prepared.filter(|_| !pb.filter_linear && !pb.stencil_write_mode) {
            static VERIFY: LazyLock<bool> =
                LazyLock::new(|| std::env::var("BALATRO_VERIFY_RASTER").as_deref() == Ok("1"));
            static CHECKS: AtomicU64 = AtomicU64::new(0);
            let reference = VERIFY.then(|| {
                let mut reference = PixelBuffer::new(pb.width, pb.height);
                reference.copy_raster_source(pb);
                reference.draw_image_region_transformed(
                    src_pixels,
                    src_w,
                    src_x,
                    src_y,
                    src_rw,
                    src_rh,
                    (min_x, min_y, max_x, max_y),
                    [inv.a, inv.b, inv.tx, inv.c, inv.d, inv.ty],
                    color,
                    replace,
                    source_white_mask,
                    dp,
                );
                reference
            });
            prepared.draw(
                pb,
                [min_x, min_y, max_x, max_y],
                [inv.a, inv.b, inv.tx, inv.c, inv.d, inv.ty],
                replace,
            );
            if let Some(reference) = reference {
                assert!(
                    pb.pixels == reference.pixels,
                    "prepared card differs from the shader draw"
                );
                let count = CHECKS.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 128 == 0 {
                    eprintln!("[prepared-card-check] {count} draws matched");
                }
            }
        } else {
            pb.draw_image_region_transformed(
                src_pixels,
                src_w,
                src_x,
                src_y,
                src_rw,
                src_rh,
                (min_x, min_y, max_x, max_y),
                [inv.a, inv.b, inv.tx, inv.c, inv.d, inv.ty],
                color,
                replace,
                source_white_mask,
                dp,
            );
        }
        if let Some(started) = started {
            let elapsed = started.elapsed().as_nanos() as u64;
            ROTATED_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
            ROTATED_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            if dp.dissolve > 0.01 || dp.shader_effect != 0 {
                EFFECT_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                EFFECT_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            }
        }
    } else {
        let (dst_x, dst_y) = (composite.tx, composite.ty);
        let (final_sx, final_sy) = (composite.a, composite.d);
        let started = PROFILE_ENABLED.then(Instant::now);
        let mut profiled_pixels = 0;
        if *PROFILE_ENABLED {
            let dst_w = (src_rw * final_sx.abs()).ceil().max(0.0) as u64;
            let dst_h = (src_rh * final_sy.abs()).ceil().max(0.0) as u64;
            profiled_pixels = dst_w.saturating_mul(dst_h);
            AXIS_DRAW_PIXELS.fetch_add(profiled_pixels, Ordering::Relaxed);
        }
        pb.draw_image_region(
            src_pixels,
            src_w,
            src_h,
            src_x,
            src_y,
            src_rw,
            src_rh,
            dst_x,
            dst_y,
            final_sx,
            final_sy,
            color,
            replace,
            source_white_mask,
            dp,
        );
        if let Some(started) = started {
            let elapsed = started.elapsed().as_nanos() as u64;
            AXIS_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
            AXIS_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);

            if source_white_mask {
                MASK_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                MASK_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
                MASK_DRAW_PIXELS.fetch_add(profiled_pixels, Ordering::Relaxed);
            }

            if dp.dissolve > 0.01 || dp.shader_effect != 0 {
                EFFECT_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                EFFECT_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            }

            let abs_sx = final_sx.abs();
            let abs_sy = final_sy.abs();
            let non_identity = abs_sx < 0.99 || abs_sx > 1.01 || abs_sy < 0.99 || abs_sy > 1.01;
            let large_downscale = abs_sx > 0.0
                && abs_sy > 0.0
                && (1.0 / abs_sx > 2.0 || 1.0 / abs_sy > 2.0)
                && src_rw * abs_sx > 3.0
                && src_rh * abs_sy > 3.0;
            if pb.filter_linear && non_identity && large_downscale {
                BOX_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                BOX_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            } else if pb.filter_linear && non_identity {
                LINEAR_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                LINEAR_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            } else {
                NEAREST_DRAW_CALLS.fetch_add(1, Ordering::Relaxed);
                NEAREST_DRAW_NS.fetch_add(elapsed, Ordering::Relaxed);
            }
        }
    }
}

pub(super) fn draw_region(
    state: &SharedState,
    src_pixels: &[u8],
    src_w: u32,
    src_h: u32,
    src_x: f32,
    src_y: f32,
    src_rw: f32,
    src_rh: f32,
    x: f32,
    y: f32,
    r: f32,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    t: &Transform,
    color: [u8; 4],
    replace: bool,
    source_white_mask: bool,
    dp: DissolveParams,
) {
    state.with_active_buffer(|pb| {
        draw_region_to_buf(
            pb,
            src_pixels,
            src_w,
            src_h,
            src_x,
            src_y,
            src_rw,
            src_rh,
            x,
            y,
            r,
            sx,
            sy,
            ox,
            oy,
            t,
            color,
            replace,
            source_white_mask,
            dp,
            None,
        );
    });
}
