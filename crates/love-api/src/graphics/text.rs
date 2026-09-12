// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

/// Parse font args: newFont(size) or newFont(path, size) → (Option<path>, size)
pub(super) fn parse_font_args(args: &LuaMultiValue) -> (Option<String>, f32) {
    let mut iter = args.iter();
    match iter.next() {
        Some(LuaValue::Number(n)) => (None, *n as f32),
        Some(LuaValue::Integer(n)) => (None, *n as f32),
        Some(LuaValue::String(s)) => {
            let path = s.to_string_lossy().to_string();
            let size = match iter.next() {
                Some(LuaValue::Number(n)) => *n as f32,
                Some(LuaValue::Integer(n)) => *n as f32,
                _ => 12.0,
            };
            (Some(path), size)
        }
        _ => (None, 12.0),
    }
}

/// Load a font from the game source, or create a fallback.
/// Returns the font_id in the font registry.
pub(super) fn load_font(state: &SharedState, path: Option<&str>, size: f32) -> u64 {
    if let Some(font_path) = path {
        if let Some(font_id) = state.font_paths.lock().get(font_path).copied() {
            return font_id;
        }

        let font_id = {
            let mut id = state.next_font_id.lock();
            let fid = *id;
            *id += 1;
            fid
        };
        state.fonts.lock().insert(
            font_id,
            std::sync::Arc::new(FontData::new(
                std::sync::Arc::clone(&state.game_source),
                font_path.to_owned(),
                size,
                Arc::clone(&state.font_cache),
            )),
        );
        state
            .font_paths
            .lock()
            .insert(font_path.to_owned(), font_id);
        return font_id;
    }
    // Return 0 = no loaded font, will fall back to bitmap
    0
}

/// Get a FontData by ID, or None for bitmap fallback
pub(super) fn get_font(state: &SharedState, font_id: u64) -> Option<std::sync::Arc<FontData>> {
    if font_id == 0 {
        return None;
    }
    state.fonts.lock().get(&font_id).cloned()
}

pub(super) fn new_font_table(
    lua: &Lua,
    state: &SharedState,
    font_id: u64,
    size: f32,
) -> LuaResult<LuaValue> {
    let font = lua.create_table()?;
    font.set("_size", size)?;
    font.set("_font_id", font_id)?;

    let font_data = get_font(state, font_id);
    {
        let fd = font_data.clone();
        font.set(
            "getHeight",
            lua.create_function(move |_, _self: LuaValue| {
                Ok(fd
                    .as_ref()
                    .map_or(size, |font| font.line_height_at(size).ceil()))
            })?,
        )?;
    }

    {
        let fd = font_data.clone();
        font.set(
            "getWidth",
            lua.create_function(move |_, (_self, text): (LuaValue, String)| match &fd {
                Some(f) => Ok(f.text_width_at(&text, size)),
                None => Ok(text.len() as f32 * (size * 0.6).ceil()),
            })?,
        )?;
    }

    font.set(
        "getBaseline",
        lua.create_function(move |_, _self: LuaValue| Ok(size * 0.75))?,
    )?;
    font.set(
        "getAscent",
        lua.create_function(move |_, _self: LuaValue| Ok(size * 0.75))?,
    )?;
    font.set(
        "getDescent",
        lua.create_function(move |_, _self: LuaValue| Ok(size * 0.25))?,
    )?;
    font.set(
        "getLineHeight",
        lua.create_function(|_, _self: LuaValue| Ok(1.0f32))?,
    )?;
    font.set(
        "setLineHeight",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;

    {
        let fd = font_data.clone();
        font.set(
            "getWrap",
            lua.create_function(move |lua, (_self, text, limit): (LuaValue, String, f32)| {
                let char_w_fn = |s: &str| -> f32 {
                    match &fd {
                        Some(f) => f.text_width_at(s, size),
                        None => s.len() as f32 * (size * 0.6).ceil(),
                    }
                };

                let mut lines_vec: Vec<String> = Vec::new();
                let mut current_line = String::new();
                let mut current_width: f32 = 0.0;

                for word in text.split_whitespace() {
                    let word_width = char_w_fn(word);
                    if current_width + word_width > limit && !current_line.is_empty() {
                        lines_vec.push(current_line.clone());
                        current_line.clear();
                        current_width = 0.0;
                    }
                    if !current_line.is_empty() {
                        current_line.push(' ');
                        current_width += char_w_fn(" ");
                    }
                    current_line.push_str(word);
                    current_width += word_width;
                }
                if !current_line.is_empty() {
                    lines_vec.push(current_line);
                }

                let max_width = lines_vec
                    .iter()
                    .map(|l| char_w_fn(l))
                    .fold(0.0f32, f32::max);
                let lines = lua.create_table()?;
                for (i, line) in lines_vec.iter().enumerate() {
                    lines.set(i + 1, line.as_str())?;
                }
                Ok((max_width.min(limit), lines))
            })?,
        )?;
    }

    font.set(
        "setFilter",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    font.set(
        "getFilter",
        lua.create_function(|_, _self: LuaValue| Ok(("nearest", "nearest")))?,
    )?;
    font.set(
        "setFallbacks",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    font.set(
        "hasGlyphs",
        lua.create_function(|_, _args: LuaMultiValue| Ok(true))?,
    )?;
    font.set(
        "type",
        lua.create_function(|_, _self: LuaValue| Ok("Font"))?,
    )?;
    font.set(
        "typeOf",
        lua.create_function(|_, (_self, t): (LuaValue, String)| Ok(t == "Font" || t == "Object"))?,
    )?;
    Ok(LuaValue::Table(font))
}

/// Render text string into a new image in the image registry.
/// Uses the active font if available, otherwise falls back to bitmap.
/// Returns (image_id, width, height).
pub(super) fn render_lua_text(
    lua: &Lua,
    state: &SharedState,
    value: Option<&LuaValue>,
    font_size: f32,
    font_id: u64,
) -> LuaResult<(u64, i32, i32)> {
    let Some(value) = value else {
        return Ok((0, 0, 0));
    };
    let segments = parse_colored_text(lua, value)?;
    let plain = if segments.is_none() {
        extract_text_from_lua(lua, value)?
    } else {
        String::new()
    };
    let previous_font = *state.active_font_id.lock();
    *state.active_font_id.lock() = font_id;
    let result = match segments {
        Some(segments) => render_colored_text_to_image(state, &segments, font_size),
        None => render_text_to_image(state, &plain, font_size),
    };
    *state.active_font_id.lock() = previous_font;
    Ok(result)
}

pub(super) fn render_text_to_image(
    state: &SharedState,
    text: &str,
    font_size: f32,
) -> (u64, i32, i32) {
    if text.is_empty() {
        return (0, 0, 0);
    }

    // Try to use TTF font
    let font_id = *state.active_font_id.lock();
    if let Some(fd) = get_font(state, font_id) {
        let image = state
            .text_cache
            .lock()
            .rasterize(font_id, font_size, text, || {
                let (width, height, pixels) = fd.rasterize_text_at(text, font_size);
                ImageData {
                    width,
                    height,
                    pixels,
                    white_alpha_mask: true,
                }
            });
        let (w, h) = (image.width, image.height);
        if w == 0 || h == 0 {
            return (0, 0, 0);
        }
        let image_id = {
            let mut id = state.next_image_id.lock();
            let iid = *id;
            *id += 1;
            iid
        };
        state.images.lock().insert(image_id, image);
        return (image_id, w as i32, h as i32);
    }

    // Bitmap fallback
    let scale = (font_size / 8.0).max(1.0);
    let char_w = (8.0 * scale) as u32;
    let char_h = (8.0 * scale) as u32;
    let tw = (text.len() as u32 * char_w).min(4096);
    let th = char_h.min(256);

    if tw == 0 || th == 0 {
        return (0, 0, 0);
    }

    let mut pb = PixelBuffer::new(tw, th);
    let mut cx: u32 = 0;
    for ch in text.chars() {
        if cx >= tw {
            break;
        }
        let glyph_idx = (ch as usize).min(127);
        let glyph = &sprite_to_text::pixel_buffer::FONT_8X8[glyph_idx];
        for row in 0..8u32 {
            let byte = glyph[row as usize];
            for col in 0..8u32 {
                if byte & (0x80 >> col) != 0 {
                    let px_start = cx + (col as f32 * scale) as u32;
                    let py_start = (row as f32 * scale) as u32;
                    let px_end = (cx + ((col + 1) as f32 * scale) as u32).min(tw);
                    let py_end = (((row + 1) as f32 * scale) as u32).min(th);
                    for py in py_start..py_end {
                        for px in px_start..px_end {
                            pb.set_pixel(px, py, 255, 255, 255, 255);
                        }
                    }
                }
            }
        }
        cx += char_w;
    }

    let image_id = {
        let mut id = state.next_image_id.lock();
        let iid = *id;
        *id += 1;
        iid
    };
    state.images.lock().insert(
        image_id,
        Arc::new(ImageData {
            width: tw,
            height: th,
            pixels: pb.pixels,
            white_alpha_mask: true,
        }),
    );

    (image_id, tw as i32, th as i32)
}

/// Render colored text segments to an RGBA image.
/// Each segment has its own color baked into the pixel data.
pub(super) fn render_colored_text_to_image(
    state: &SharedState,
    segments: &[ColoredSegment],
    font_size: f32,
) -> (u64, i32, i32) {
    if segments.is_empty() {
        return (0, 0, 0);
    }

    let font_id = *state.active_font_id.lock();
    if let Some(fd) = get_font(state, font_id) {
        let image = state
            .text_cache
            .lock()
            .rasterize_colored(font_id, font_size, segments, || {
                let Some(font) = fd.font() else {
                    return ImageData {
                        width: 0,
                        height: 0,
                        pixels: Vec::new(),
                        white_alpha_mask: false,
                    };
                };
                // Measure total width
                let mut total_width = 0.0f32;
                for (_, text) in segments {
                    total_width += fd.text_width_at(text, font_size);
                }
                let width = total_width.ceil() as u32;
                let line_h = fd.line_height_at(font_size).ceil() as u32;
                if width == 0 || line_h == 0 {
                    return ImageData {
                        width: 0,
                        height: 0,
                        pixels: Vec::new(),
                        white_alpha_mask: false,
                    };
                }

                let metrics = font.horizontal_line_metrics(font_size);
                let ascent = match metrics {
                    Some(m) => m.ascent.ceil() as i32,
                    None => font_size as i32,
                };

                let mut pixels = vec![0u8; (width * line_h * 4) as usize];
                let mut cursor_x = 0.0f32;

                for (color, text) in segments {
                    for ch in text.chars() {
                        let (m, bitmap) = font.rasterize(ch, font_size);
                        let glyph_x = cursor_x as i32 + m.xmin;
                        let glyph_y = ascent - m.height as i32 - m.ymin;

                        for gy in 0..m.height {
                            for gx in 0..m.width {
                                let px = glyph_x + gx as i32;
                                let py = glyph_y + gy as i32;
                                if px >= 0 && (px as u32) < width && py >= 0 && (py as u32) < line_h
                                {
                                    let alpha = bitmap[gy * m.width + gx];
                                    if alpha > 0 {
                                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                                        // Blend with existing pixel (later segments overlay)
                                        let fa = (alpha as u16 * color[3] as u16 / 255) as u8;
                                        let da = pixels[idx + 3];
                                        if da == 0 {
                                            pixels[idx] = color[0];
                                            pixels[idx + 1] = color[1];
                                            pixels[idx + 2] = color[2];
                                            pixels[idx + 3] = fa;
                                        } else {
                                            // Proper source-over alpha compositing
                                            let inv_sa = 255 - fa as u16;
                                            pixels[idx] = ((color[0] as u16 * fa as u16
                                                + pixels[idx] as u16 * inv_sa)
                                                / 255)
                                                as u8;
                                            pixels[idx + 1] = ((color[1] as u16 * fa as u16
                                                + pixels[idx + 1] as u16 * inv_sa)
                                                / 255)
                                                as u8;
                                            pixels[idx + 2] = ((color[2] as u16 * fa as u16
                                                + pixels[idx + 2] as u16 * inv_sa)
                                                / 255)
                                                as u8;
                                            pixels[idx + 3] = (fa as u16 + da as u16 * inv_sa / 255)
                                                .min(255)
                                                as u8;
                                        }
                                    }
                                }
                            }
                        }
                        cursor_x += m.advance_width;
                    }
                }
                ImageData {
                    width,
                    height: line_h,
                    pixels,
                    white_alpha_mask: false,
                }
            });
        let (width, height) = (image.width, image.height);
        if width == 0 || height == 0 {
            return (0, 0, 0);
        }
        let image_id = {
            let mut id = state.next_image_id.lock();
            let iid = *id;
            *id += 1;
            iid
        };
        state.images.lock().insert(image_id, image);
        return (image_id, width as i32, height as i32);
    }

    // Fallback: strip colors and render as plain white text
    let full_text: String = segments.iter().map(|(_, t)| t.as_str()).collect();
    render_text_to_image(state, &full_text, font_size)
}

/// Render wrapped text and return its image ID and dimensions.
pub(super) fn render_text_to_image_wrapped(
    state: &SharedState,
    text: &str,
    font_size: f32,
    wrap_limit: f32,
    align: &str,
) -> (u64, i32, i32) {
    if text.is_empty() {
        return (0, 0, 0);
    }

    let font_id = *state.active_font_id.lock();
    if let Some(fd) = get_font(state, font_id) {
        // TTF path — use fontdue for proper word wrapping and rendering
        let measure = |s: &str| -> f32 { fd.text_width_at(s, font_size) };
        let line_h = fd.line_height_at(font_size).ceil() as u32;

        // Word wrap using real font metrics
        let mut lines: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            let mut current_line = String::new();
            let mut current_width: f32 = 0.0;
            for word in paragraph.split_whitespace() {
                let word_w = measure(word);
                if current_width + word_w > wrap_limit && !current_line.is_empty() {
                    lines.push(current_line.clone());
                    current_line.clear();
                    current_width = 0.0;
                }
                if !current_line.is_empty() {
                    current_line.push(' ');
                    current_width += measure(" ");
                }
                current_line.push_str(word);
                current_width += word_w;
            }
            lines.push(current_line);
        }

        let total_h = (lines.len() as u32 * line_h).max(1);
        let total_w = wrap_limit.ceil() as u32;
        if total_w == 0 || total_h == 0 {
            return (0, 0, 0);
        }

        let mut pb = PixelBuffer::new(total_w, total_h);
        for (i, line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let (lw, lh, lpixels) = fd.rasterize_text_at(line, font_size);
            if lw == 0 || lh == 0 {
                continue;
            }
            let line_px_w = lw as f32;
            let lx = match align {
                "center" => ((total_w as f32 - line_px_w) / 2.0).max(0.0) as i32,
                "right" => (total_w as f32 - line_px_w).max(0.0) as i32,
                _ => 0,
            };
            let ly = (i as u32 * line_h) as i32;
            // Blit the rasterized line onto the buffer
            for gy in 0..lh {
                for gx in 0..lw {
                    let si = ((gy * lw + gx) * 4) as usize;
                    let alpha = lpixels[si + 3];
                    if alpha > 0 {
                        let px = lx + gx as i32;
                        let py = ly + gy as i32;
                        if px >= 0 && (px as u32) < total_w && py >= 0 && (py as u32) < total_h {
                            pb.set_pixel(px as u32, py as u32, 255, 255, 255, alpha);
                        }
                    }
                }
            }
        }

        let image_id = {
            let mut id = state.next_image_id.lock();
            let iid = *id;
            *id += 1;
            iid
        };
        state.images.lock().insert(
            image_id,
            Arc::new(ImageData {
                width: total_w,
                height: total_h,
                pixels: pb.pixels,
                white_alpha_mask: true,
            }),
        );
        return (image_id, total_w as i32, total_h as i32);
    }

    // Bitmap fallback
    let scale = (font_size / 8.0).max(1.0);
    let char_w = (8.0 * scale).ceil();
    let line_h = (8.0 * scale).ceil() as u32;

    let lines = word_wrap(text, char_w, wrap_limit);
    let total_h = (lines.len() as u32 * line_h).max(1);
    let total_w = wrap_limit.ceil() as u32;

    if total_w == 0 || total_h == 0 {
        return (0, 0, 0);
    }

    let mut pb = PixelBuffer::new(total_w, total_h);
    for (i, line) in lines.iter().enumerate() {
        let ly = (i as u32 * line_h) as i32;
        let line_px_w = line.len() as f32 * char_w;
        let lx = match align {
            "center" => ((total_w as f32 - line_px_w) / 2.0).max(0.0) as i32,
            "right" => (total_w as f32 - line_px_w).max(0.0) as i32,
            _ => 0,
        };
        pb.draw_text_scaled(line, lx, ly, scale, [255, 255, 255, 255]);
    }

    let image_id = {
        let mut id = state.next_image_id.lock();
        let iid = *id;
        *id += 1;
        iid
    };
    state.images.lock().insert(
        image_id,
        Arc::new(ImageData {
            width: total_w,
            height: total_h,
            pixels: pb.pixels,
            white_alpha_mask: true,
        }),
    );

    (image_id, total_w as i32, total_h as i32)
}

/// Bresenham line drawing between two points.
/// Word-wrap text to fit within a pixel width limit.
fn word_wrap(text: &str, char_w: f32, limit: f32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    // Split by explicit newlines first
    for paragraph in text.split('\n') {
        let mut current_line = String::new();
        let mut current_width: f32 = 0.0;

        for word in paragraph.split_whitespace() {
            let word_width = word.len() as f32 * char_w;
            if current_width + word_width > limit && !current_line.is_empty() {
                lines.push(current_line.clone());
                current_line.clear();
                current_width = 0.0;
            }
            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += char_w;
            }
            current_line.push_str(word);
            current_width += word_width;
        }
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub(super) fn register(lua: &Lua, g: &LuaTable, state: &Arc<SharedState>) -> LuaResult<()> {
    // love.graphics.newFont(path_or_size [, size]) -> Font
    {
        let s = Arc::clone(state);
        g.set(
            "newFont",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let (path, size) = parse_font_args(&args);
                let font_id = load_font(&s, path.as_deref(), size);
                new_font_table(lua, &s, font_id, size)
            })?,
        )?;
    }

    // love.graphics.setNewFont(path_or_size [, size]) -> Font
    {
        let s = Arc::clone(state);
        g.set(
            "setNewFont",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let (path, size) = parse_font_args(&args);
                let font_id = load_font(&s, path.as_deref(), size);
                *s.active_font_size.lock() = size;
                *s.active_font_id.lock() = font_id;
                new_font_table(lua, &s, font_id, size)
            })?,
        )?;
    }

    // love.graphics.setFont(font)
    {
        let s = Arc::clone(state);
        g.set(
            "setFont",
            lua.create_function(move |_, font: LuaValue| {
                if let LuaValue::Table(t) = font {
                    if let Ok(size) = t.get::<f32>("_size") {
                        *s.active_font_size.lock() = size;
                    }
                    if let Ok(fid) = t.get::<u64>("_font_id") {
                        *s.active_font_id.lock() = fid;
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.getFont() -> Font
    {
        let s = Arc::clone(state);
        g.set(
            "getFont",
            lua.create_function(move |lua, ()| {
                let size = *s.active_font_size.lock();
                let font_id = *s.active_font_id.lock();
                new_font_table(lua, &s, font_id, size)
            })?,
        )?;
    }

    // love.graphics.newText(font, text) -> Text
    {
        let s = Arc::clone(state);
        g.set(
            "newText",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let (font_size, text_font_id) = match args.get(0) {
                    Some(LuaValue::Table(t)) => {
                        let sz = t.get::<f32>("_size").unwrap_or(12.0);
                        let fid = t.get::<u64>("_font_id").unwrap_or(0);
                        (sz, fid)
                    }
                    _ => (12.0, 0u64),
                };
                let t = lua.create_table()?;
                let (image_id, tw, th) =
                    render_lua_text(lua, &s, args.get(1), font_size, text_font_id)?;

                t.set("_image_id", image_id)?;
                t.set("_text_w", tw)?;
                t.set("_text_h", th)?;
                t.set("_font_id", text_font_id)?;
                t.set("_font_size", font_size)?;
                t.set("_is_text", true)?;
                let current_image_id = Arc::new(AtomicU64::new(image_id));

                {
                    let sr = Arc::clone(&s);
                    let guard_id = Arc::clone(&current_image_id);
                    t.set(
                        "set",
                        lua.create_function(move |lua, args: LuaMultiValue| {
                            let self_tbl = match args.get(0) {
                                Some(LuaValue::Table(t)) => t.clone(),
                                _ => return Ok(()),
                            };
                            let fs = self_tbl.get::<f32>("_font_size").unwrap_or(12.0);
                            let fid = self_tbl.get::<u64>("_font_id").unwrap_or(0);
                            let (new_id, nw, nh) = render_lua_text(lua, &sr, args.get(1), fs, fid)?;
                            let old_id = guard_id.swap(new_id, Ordering::Relaxed);
                            if old_id != 0 {
                                sr.images.lock().remove(&old_id);
                            }
                            self_tbl.set("_image_id", new_id).ok();
                            self_tbl.set("_text_w", nw).ok();
                            self_tbl.set("_text_h", nh).ok();
                            Ok(())
                        })?,
                    )?;
                }
                {
                    let sr2 = Arc::clone(&s);
                    let guard_id = Arc::clone(&current_image_id);
                    t.set(
                        "setf",
                        lua.create_function(move |lua, args: LuaMultiValue| {
                            let self_tbl = match args.get(0) {
                                Some(LuaValue::Table(t)) => t.clone(),
                                _ => return Ok(()),
                            };
                            let text = match args.get(1) {
                                Some(value) => extract_text_from_lua(lua, value)?,
                                None => String::new(),
                            };
                            let wrap_limit = parse_num_arg(args.get(2), 400.0);
                            let align = match args.get(3) {
                                Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                                _ => "left".to_string(),
                            };
                            let fs = self_tbl.get::<f32>("_font_size").unwrap_or(12.0);
                            let fid = self_tbl.get::<u64>("_font_id").unwrap_or(0);
                            let prev = *sr2.active_font_id.lock();
                            *sr2.active_font_id.lock() = fid;
                            let (new_id, nw, nh) =
                                render_text_to_image_wrapped(&sr2, &text, fs, wrap_limit, &align);
                            *sr2.active_font_id.lock() = prev;
                            let old_id = guard_id.swap(new_id, Ordering::Relaxed);
                            if old_id != 0 {
                                sr2.images.lock().remove(&old_id);
                            }
                            self_tbl.set("_image_id", new_id).ok();
                            self_tbl.set("_text_w", nw).ok();
                            self_tbl.set("_text_h", nh).ok();
                            Ok(())
                        })?,
                    )?;
                }
                {
                    let sr = Arc::clone(&s);
                    let guard_id = Arc::clone(&current_image_id);
                    t.set(
                        "release",
                        lua.create_function(move |_, self_tbl: LuaTable| {
                            let id = guard_id.swap(0, Ordering::Relaxed);
                            if id != 0 {
                                sr.images.lock().remove(&id);
                            }
                            self_tbl.set("_image_id", 0).ok();
                            self_tbl.set("_gc_guard", LuaValue::Nil).ok();
                            Ok(())
                        })?,
                    )?;
                }
                t.set(
                    "getWidth",
                    lua.create_function(|_, self_tbl: LuaTable| {
                        Ok(self_tbl.get::<i32>("_text_w").unwrap_or(0))
                    })?,
                )?;
                t.set(
                    "getHeight",
                    lua.create_function(|_, self_tbl: LuaTable| {
                        Ok(self_tbl.get::<i32>("_text_h").unwrap_or(0))
                    })?,
                )?;
                t.set(
                    "getDimensions",
                    lua.create_function(|_, self_tbl: LuaTable| {
                        Ok((
                            self_tbl.get::<i32>("_text_w").unwrap_or(0),
                            self_tbl.get::<i32>("_text_h").unwrap_or(0),
                        ))
                    })?,
                )?;
                t.set("_font_size", font_size)?;
                t.set(
                    "type",
                    lua.create_function(|_, _self: LuaValue| Ok("Text"))?,
                )?;
                t.set(
                    "typeOf",
                    lua.create_function(|_, (_self, t): (LuaValue, String)| {
                        Ok(t == "Text" || t == "Drawable" || t == "Object")
                    })?,
                )?;
                let guard = lua.create_userdata(TextImageGuard {
                    id: current_image_id,
                    state: Arc::clone(&s),
                })?;
                t.set("_gc_guard", guard)?;
                Ok(LuaValue::Table(t))
            })?,
        )?;
    }

    Ok(())
}
