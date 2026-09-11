use super::*;

fn polygon_vertices(table: &LuaTable) -> LuaResult<Vec<(f32, f32)>> {
    let coordinate_count = table.raw_len();
    if coordinate_count < 6 || coordinate_count % 2 != 0 {
        return Ok(Vec::new());
    }
    let mut vertices = Vec::with_capacity(coordinate_count / 2);
    for index in (1..=coordinate_count).step_by(2) {
        vertices.push((table.get(index)?, table.get(index + 1)?));
    }
    Ok(vertices)
}

fn draw_polygon_vertices(state: &SharedState, vertices: &[(f32, f32)], fill: bool) {
    if vertices.len() < 3 {
        return;
    }

    let color = color_f32_to_u8(*state.current_color.lock());
    let transform = current_transform(state);
    let mut stack_vertices = [(0.0, 0.0); 64];
    let transformed = if vertices.len() <= stack_vertices.len() {
        for (target, &(x, y)) in stack_vertices.iter_mut().zip(vertices) {
            *target = transform.apply(x, y);
        }
        &stack_vertices[..vertices.len()]
    } else {
        return draw_large_polygon(state, vertices, fill, color, transform);
    };

    draw_transformed_polygon(state, transformed, fill, color, transform);
}

fn draw_large_polygon(
    state: &SharedState,
    vertices: &[(f32, f32)],
    fill: bool,
    color: [u8; 4],
    transform: Transform,
) {
    let transformed: Vec<_> = vertices
        .iter()
        .map(|&(x, y)| transform.apply(x, y))
        .collect();
    draw_transformed_polygon(state, &transformed, fill, color, transform);
}

fn draw_transformed_polygon(
    state: &SharedState,
    transformed: &[(f32, f32)],
    fill: bool,
    color: [u8; 4],
    transform: Transform,
) {
    let line_width = *state.line_width.lock();
    draw_transformed_polygon_with_width(state, transformed, fill, color, transform, line_width);
}

fn draw_transformed_polygon_with_width(
    state: &SharedState,
    transformed: &[(f32, f32)],
    fill: bool,
    color: [u8; 4],
    transform: Transform,
    line_width: f32,
) {
    let (scale_x, _) = transform.scale_factor();
    let scaled_line_width = (line_width * scale_x).max(1.0);
    let deferred_vertices = transformed.to_vec();
    if defer_screen_draw(state, move |buffer| {
        if fill {
            fill_polygon(buffer, &deferred_vertices, color);
        } else {
            for index in 0..deferred_vertices.len() {
                let next = (index + 1) % deferred_vertices.len();
                draw_thick_line(
                    buffer,
                    deferred_vertices[index].0,
                    deferred_vertices[index].1,
                    deferred_vertices[next].0,
                    deferred_vertices[next].1,
                    scaled_line_width,
                    color,
                );
            }
        }
    }) {
        return;
    }
    state.with_active_buffer(|buffer| {
        if fill {
            fill_polygon(buffer, transformed, color);
        } else {
            for index in 0..transformed.len() {
                let next = (index + 1) % transformed.len();
                draw_thick_line(
                    buffer,
                    transformed[index].0,
                    transformed[index].1,
                    transformed[next].0,
                    transformed[next].1,
                    scaled_line_width,
                    color,
                );
            }
        }
    });
}

fn draw_ui_polygon(
    state: &SharedState,
    vertices: &[(f32, f32)],
    fill: bool,
    color: [u8; 4],
    transform: Transform,
    line_width: f32,
) {
    if vertices.len() < 3 {
        return;
    }

    let mut transformed = [(0.0, 0.0); 64];
    if vertices.len() > transformed.len() {
        return;
    }
    for (target, &(x, y)) in transformed.iter_mut().zip(vertices) {
        *target = transform.apply(x, y);
    }
    draw_transformed_polygon_with_width(
        state,
        &transformed[..vertices.len()],
        fill,
        color,
        transform,
        line_width,
    );
}

fn draw_ui_batch(buffer: &mut PixelBuffer, draws: &[UiBatchDraw]) {
    let mut transformed = [(0.0, 0.0); 64];
    let mut simple_fills = 0;
    let mut scanline_fills = 0;
    let mut scanline_axis = 0;
    let mut scanline_tiny_rotation = 0;
    let mut scanline_small_rotation = 0;
    let mut scanline_large_rotation = 0;
    let mut polygon_lines = 0;
    let mut text_draws = 0;
    for draw in draws {
        match draw {
            UiBatchDraw::Polygon {
                vertices,
                transform,
                fill,
                color,
                line_width,
            } => {
                for (target, &(x, y)) in transformed.iter_mut().zip(vertices.iter()) {
                    *target = transform.apply(x, y);
                }
                let vertices = &transformed[..vertices.len()];
                if *fill {
                    if fill_polygon(buffer, vertices, *color) {
                        simple_fills += 1;
                    } else {
                        scanline_fills += 1;
                        let rotation = transform.c.atan2(transform.a).abs();
                        if rotation <= f32::EPSILON {
                            scanline_axis += 1;
                        } else if rotation <= 0.001 {
                            scanline_tiny_rotation += 1;
                        } else if rotation <= 0.01 {
                            scanline_small_rotation += 1;
                        } else {
                            scanline_large_rotation += 1;
                        }
                    }
                } else {
                    polygon_lines += 1;
                    for index in 0..vertices.len() {
                        let next = (index + 1) % vertices.len();
                        draw_thick_line(
                            buffer,
                            vertices[index].0,
                            vertices[index].1,
                            vertices[next].0,
                            vertices[next].1,
                            *line_width,
                            *color,
                        );
                    }
                }
            }
            UiBatchDraw::Text {
                image,
                transform,
                x,
                y,
                scale_x,
                scale_y,
                color,
                replace,
            } => {
                text_draws += 1;
                draw_region_to_buf(
                    buffer,
                    &image.pixels,
                    image.width,
                    image.height,
                    0.0,
                    0.0,
                    image.width as f32,
                    image.height as f32,
                    *x,
                    *y,
                    0.0,
                    *scale_x,
                    *scale_y,
                    0.0,
                    0.0,
                    transform,
                    *color,
                    *replace,
                    image.white_alpha_mask,
                    DissolveParams::NONE,
                    None,
                )
            }
        }
    }
    if *PROFILE_ENABLED {
        UI_SIMPLE_FILLS.fetch_add(simple_fills, Ordering::Relaxed);
        UI_SCANLINE_FILLS.fetch_add(scanline_fills, Ordering::Relaxed);
        UI_SCANLINE_AXIS.fetch_add(scanline_axis, Ordering::Relaxed);
        UI_SCANLINE_TINY_ROTATION.fetch_add(scanline_tiny_rotation, Ordering::Relaxed);
        UI_SCANLINE_SMALL_ROTATION.fetch_add(scanline_small_rotation, Ordering::Relaxed);
        UI_SCANLINE_LARGE_ROTATION.fetch_add(scanline_large_rotation, Ordering::Relaxed);
        UI_POLYGON_LINES.fetch_add(polygon_lines, Ordering::Relaxed);
        UI_TEXT_DRAWS.fetch_add(text_draws, Ordering::Relaxed);
    }
}

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.polygon(mode, vertices...)
    {
        let s = Arc::clone(state);
        g.set(
            "polygon",
            lua.create_function(move |_, args: LuaMultiValue| {
                let _profile = ProfileTimer::start(&POLYGON_CALLS, &POLYGON_NS);
                if args.len() < 2 {
                    return Ok(());
                }
                let mode = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => return Ok(()),
                };

                let vertices = match args.get(1) {
                    Some(LuaValue::Table(table)) => polygon_vertices(table)?,
                    _ => {
                        let mut coordinates = Vec::with_capacity(args.len().saturating_sub(1));
                        for value in args.iter().skip(1) {
                            match value {
                                LuaValue::Number(number) => coordinates.push(*number as f32),
                                LuaValue::Integer(number) => coordinates.push(*number as f32),
                                _ => break,
                            }
                        }
                        if coordinates.len() < 6 || coordinates.len() % 2 != 0 {
                            Vec::new()
                        } else {
                            coordinates
                                .chunks_exact(2)
                                .map(|pair| (pair[0], pair[1]))
                                .collect()
                        }
                    }
                };
                if vertices.is_empty() {
                    return Ok(());
                }
                draw_polygon_vertices(&s, &vertices, mode == "fill");
                Ok(())
            })?,
        )?;
    }

    g.set(
        "newPolygonData",
        lua.create_function(|lua, vertices: LuaTable| {
            lua.create_userdata(PolygonData {
                vertices: polygon_vertices(&vertices)?.into(),
            })
        })?,
    )?;

    {
        let s = Arc::clone(state);
        g.set(
            "drawPolygonData",
            lua.create_function(move |_, (mode, data): (String, LuaAnyUserData)| {
                let _profile = ProfileTimer::start(&POLYGON_CALLS, &POLYGON_NS);
                let data = data.borrow::<PolygonData>()?;
                draw_polygon_vertices(&s, &data.vertices, mode == "fill");
                Ok(())
            })?,
        )?;
    }

    {
        let s = Arc::clone(state);
        g.set(
            "_drawUIPolygon",
            lua.create_function(
                move |_,
                      (
                    data,
                    fill,
                    world_scale,
                    x,
                    y,
                    angle,
                    origin_x,
                    origin_y,
                    object_scale,
                    local_scale,
                    colour,
                    line_width,
                ): (
                    LuaAnyUserData,
                    bool,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    LuaTable,
                    f32,
                )| {
                    let data = data.borrow::<PolygonData>()?;
                    let mut transform = prepare_draw_transform(
                        current_transform(&s),
                        world_scale,
                        x,
                        y,
                        angle,
                        origin_x,
                        origin_y,
                        object_scale,
                    );
                    transform.scale(local_scale, local_scale);
                    draw_ui_polygon(
                        &s,
                        &data.vertices,
                        fill,
                        color_f32_to_u8(read_shader_colour(&colour)),
                        transform,
                        line_width,
                    );
                    Ok(())
                },
            )?,
        )?;
    }

    {
        let s = Arc::clone(state);
        g.set(
            "_drawUIText",
            lua.create_function(
                move |_,
                      (
                    drawable,
                    colour,
                    world_scale,
                    x,
                    y,
                    angle,
                    origin_x,
                    origin_y,
                    object_scale,
                    vertical,
                    vertical_height,
                    local_x,
                    local_y,
                    scale_x,
                    scale_y,
                ): (
                    LuaTable,
                    LuaTable,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    bool,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                )| {
                    let _profile = ProfileTimer::start(&DRAW_CALLS, &DRAW_NS);
                    let Some(image) = drawable_image(&s, &drawable)? else {
                        return Ok(());
                    };
                    let mut transform = prepare_draw_transform(
                        current_transform(&s),
                        world_scale,
                        x,
                        y,
                        angle,
                        origin_x,
                        origin_y,
                        object_scale,
                    );
                    if vertical {
                        transform.translate(0.0, vertical_height);
                        transform.rotate(-std::f32::consts::FRAC_PI_2);
                    }
                    let color = color_f32_to_u8(read_shader_colour(&colour));
                    let replace = *s.blend_mode.lock() == BlendMode::Replace;
                    let deferred_image = Arc::clone(&image);
                    let deferred_transform = transform.clone();
                    if defer_screen_draw(&s, move |buffer| {
                        draw_region_to_buf(
                            buffer,
                            &deferred_image.pixels,
                            deferred_image.width,
                            deferred_image.height,
                            0.0,
                            0.0,
                            deferred_image.width as f32,
                            deferred_image.height as f32,
                            local_x,
                            local_y,
                            0.0,
                            scale_x,
                            scale_y,
                            0.0,
                            0.0,
                            &deferred_transform,
                            color,
                            replace,
                            deferred_image.white_alpha_mask,
                            DissolveParams::NONE,
                            None,
                        );
                    }) {
                        return Ok(());
                    }
                    draw_region(
                        &s,
                        &image.pixels,
                        image.width,
                        image.height,
                        0.0,
                        0.0,
                        image.width as f32,
                        image.height as f32,
                        local_x,
                        local_y,
                        0.0,
                        scale_x,
                        scale_y,
                        0.0,
                        0.0,
                        &transform,
                        color,
                        replace,
                        image.white_alpha_mask,
                        DissolveParams::NONE,
                    );
                    Ok(())
                },
            )?,
        )?;
    }

    {
        let s = Arc::clone(state);
        g.set(
            "_drawUIBatch",
            lua.create_function(move |lua, (commands, count): (LuaTable, usize)| {
                let _profile = ProfileTimer::start(&DRAW_CALLS, &DRAW_NS);
                let base_transform = current_transform(&s);
                let replace = *s.blend_mode.lock() == BlendMode::Replace;
                let mut draws = Vec::with_capacity(count);

                for index in 1..=count {
                    let command: LuaTable = commands.get(index)?;
                    let cached = command.get::<LuaAnyUserData>("_native_ui_draw").ok();
                    if let Some(cached) = cached
                        .as_ref()
                        .filter(|_| !command.get::<bool>("_native_ui_dirty").unwrap_or(false))
                    {
                        if *PROFILE_ENABLED {
                            UI_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                        }
                        draws.push(cached.borrow::<CachedUiBatchDraw>()?.0.clone());
                        continue;
                    }

                    let cache_static = command.get::<bool>("_svmm_static").unwrap_or(false);
                    if cache_static && *PROFILE_ENABLED {
                        UI_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                    }
                    let draw = match command.get::<u8>(1)? {
                        0 => {
                            let data: LuaAnyUserData = command.get(2)?;
                            let data = data.borrow::<PolygonData>()?;
                            if data.vertices.len() < 3 || data.vertices.len() > 64 {
                                continue;
                            }
                            let mut transform = prepare_draw_transform(
                                base_transform.clone(),
                                command.get(4)?,
                                command.get(5)?,
                                command.get(6)?,
                                command.get(7)?,
                                command.get(8)?,
                                command.get(9)?,
                                command.get(10)?,
                            );
                            let local_scale: f32 = command.get(11)?;
                            transform.scale(local_scale, local_scale);
                            let (scale_x, _) = transform.scale_factor();
                            Some(UiBatchDraw::Polygon {
                                vertices: Arc::clone(&data.vertices),
                                transform,
                                fill: command.get(3)?,
                                color: color_f32_to_u8([
                                    command.get(12)?,
                                    command.get(13)?,
                                    command.get(14)?,
                                    command.get(15)?,
                                ]),
                                line_width: (command.get::<f32>(16)? * scale_x).max(1.0),
                            })
                        }
                        1 => {
                            let drawable: LuaTable = command.get(2)?;
                            let Some(image) = drawable_image(&s, &drawable)? else {
                                continue;
                            };
                            let mut transform = prepare_draw_transform(
                                base_transform.clone(),
                                command.get(4)?,
                                command.get(5)?,
                                command.get(6)?,
                                command.get(7)?,
                                command.get(8)?,
                                command.get(9)?,
                                command.get(10)?,
                            );
                            if command.get::<bool>(3)? {
                                transform.translate(0.0, command.get(11)?);
                                transform.rotate(-std::f32::consts::FRAC_PI_2);
                            }
                            Some(UiBatchDraw::Text {
                                image,
                                transform,
                                x: command.get(12)?,
                                y: command.get(13)?,
                                scale_x: command.get(14)?,
                                scale_y: command.get(15)?,
                                color: color_f32_to_u8([
                                    command.get(16)?,
                                    command.get(17)?,
                                    command.get(18)?,
                                    command.get(19)?,
                                ]),
                                replace,
                            })
                        }
                        _ => None,
                    };
                    if let Some(draw) = draw {
                        if cache_static {
                            if let Some(cached) = &cached {
                                cached.borrow_mut::<CachedUiBatchDraw>()?.0 = draw.clone();
                            } else {
                                command.set(
                                    "_native_ui_draw",
                                    lua.create_userdata(CachedUiBatchDraw(draw.clone()))?,
                                )?;
                            }
                            command.set("_native_ui_dirty", false)?;
                        }
                        draws.push(draw);
                    }
                }

                if draws.is_empty() {
                    return Ok(());
                }
                let draws = Arc::new(draws);
                let deferred = Arc::clone(&draws);
                if defer_screen_draw(&s, move |buffer| draw_ui_batch(buffer, &deferred)) {
                    return Ok(());
                }
                s.with_active_buffer(|buffer| draw_ui_batch(buffer, &draws));
                Ok(())
            })?,
        )?;
    }

    Ok(())
}
