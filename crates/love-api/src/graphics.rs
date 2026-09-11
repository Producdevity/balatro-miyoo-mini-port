// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

mod draw;
mod ui;
use draw::{current_transform, draw_region, draw_region_to_buf, prepare_draw_transform};
mod canvas;
mod images;
mod shaders;
mod shapes;
mod sprite_batch;
mod state_bindings;
mod text;

use images::create_image_data_table;
use mlua::prelude::*;
use parking_lot::Mutex;
use shapes::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use crate::lua_util::{color_f32_to_u8, parse_color};
use crate::state::{BlendMode, FontData, ImageData, SavedGraphicsState, SharedState, Transform};
use crate::text_value::{extract_text_from_lua, parse_colored_text, ColoredSegment};
use sprite_to_text::pixel_buffer::{DissolveParams, PixelBuffer, StencilCompare};

mod profile;
use profile::*;
pub use profile::{take_draw_profile, DrawProfile};

#[inline]
fn read_shader_colour(table: &LuaTable) -> [f32; 4] {
    [
        table.get(1).unwrap_or(0.0),
        table.get(2).unwrap_or(0.0),
        table.get(3).unwrap_or(0.0),
        table.get(4).unwrap_or(1.0),
    ]
}

fn card_shader_for_source(source: &str) -> u8 {
    let filename = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let name = filename.strip_suffix(".fs").unwrap_or(filename);
    match name {
        "played" => 1,
        "debuff" => 2,
        "foil" => 3,
        "holo" => 4,
        "polychrome" => 5,
        "negative" => 6,
        "voucher" => 7,
        "booster" => 8,
        "hologram" => 9,
        "negative_shine" => 10,
        "gold_seal" => 11,
        _ if source.contains("extern") && source.contains("negative_shine") => 10,
        _ if source.contains("extern") && source.contains("gold_seal") => 11,
        _ if source.contains("extern") && source.contains("polychrome") => 5,
        _ if source.contains("extern") && source.contains("hologram") => 9,
        _ if source.contains("extern") && source.contains("played") => 1,
        _ if source.contains("extern") && source.contains("debuff") => 2,
        _ if source.contains("extern") && source.contains("foil") => 3,
        _ if source.contains("extern") && source.contains("holo") => 4,
        _ if source.contains("extern") && source.contains("negative") => 6,
        _ if source.contains("extern") && source.contains("voucher") => 7,
        _ if source.contains("extern") && source.contains("booster") => 8,
        _ => 0,
    }
}

/// Kind of resource tracked by a ResourceGuard.
#[derive(Clone, Copy)]
enum ResourceKind {
    Image,
    Canvas,
    SpriteBatch,
}

/// Guard that removes a resource from SharedState when Lua GC collects it.
/// Attached as userdata inside image/canvas/spritebatch tables.
struct ResourceGuard {
    id: u64,
    kind: ResourceKind,
    state: Arc<SharedState>,
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        match self.kind {
            ResourceKind::Image => {
                self.state.images.lock().remove(&self.id);
            }
            ResourceKind::Canvas => {
                self.state.canvases.lock().remove(&self.id);
            }
            ResourceKind::SpriteBatch => {
                self.state.sprite_batches.lock().remove(&self.id);
            }
        }
    }
}

impl LuaUserData for ResourceGuard {}

struct TextImageGuard {
    id: Arc<AtomicU64>,
    state: Arc<SharedState>,
}

impl Drop for TextImageGuard {
    fn drop(&mut self) {
        let id = self.id.swap(0, Ordering::Relaxed);
        if id != 0 {
            self.state.images.lock().remove(&id);
        }
    }
}

impl LuaUserData for TextImageGuard {}

struct PolygonData {
    vertices: Arc<[(f32, f32)]>,
}

impl LuaUserData for PolygonData {}

#[derive(Clone)]
enum UiBatchDraw {
    Polygon {
        vertices: Arc<[(f32, f32)]>,
        transform: Transform,
        fill: bool,
        color: [u8; 4],
        line_width: f32,
    },
    Text {
        image: Arc<ImageData>,
        transform: Transform,
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        color: [u8; 4],
        replace: bool,
    },
}

struct CachedUiBatchDraw(UiBatchDraw);

impl LuaUserData for CachedUiBatchDraw {}

struct ImageHandle {
    id: u64,
    data: Arc<ImageData>,
}

impl LuaUserData for ImageHandle {}

fn drawable_image(state: &SharedState, drawable: &LuaTable) -> LuaResult<Option<Arc<ImageData>>> {
    if let Ok(handle) = drawable.get::<LuaAnyUserData>("_native_image") {
        return Ok(Some(Arc::clone(&handle.borrow::<ImageHandle>()?.data)));
    }
    let image_id = drawable.get::<u64>("_image_id").unwrap_or(0);
    Ok(state.images.lock().get(&image_id).cloned())
}

struct QuadData {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[cfg(feature = "layer-pairs")]
#[path = "image_draw.rs"]
mod image_draw;
#[cfg(feature = "layer-pairs")]
pub(crate) use image_draw::ImageDraw;

#[derive(Clone, Copy)]
struct BufferedDrawState {
    blend: u8,
    filter_linear: bool,
    scissor: Option<(i32, i32, u32, u32)>,
    stencil_compare: StencilCompare,
    stencil_ref: u8,
}

impl BufferedDrawState {
    fn apply(self, buffer: &mut PixelBuffer) {
        buffer.blend = self.blend;
        buffer.filter_linear = self.filter_linear;
        buffer.scissor = self.scissor;
        buffer.stencil_compare = self.stencil_compare;
        buffer.stencil_ref = self.stencil_ref;
    }
}

fn defer_screen_draw<F>(state: &SharedState, draw: F) -> bool
where
    F: Fn(&mut PixelBuffer) + Send + 'static,
{
    defer_covered_draw(state, crate::occlusion::Coverage::default(), draw)
}

fn defer_covered_draw<F>(state: &SharedState, coverage: crate::occlusion::Coverage, draw: F) -> bool
where
    F: Fn(&mut PixelBuffer) + Send + 'static,
{
    if !state.can_defer_render()
        || *state.active_canvas.lock() != 0
        || *state.stencil_compare.lock() != StencilCompare::Disabled
    {
        return false;
    }

    let draw_state = BufferedDrawState {
        blend: state.blend_code(),
        filter_linear: *state.default_filter_linear.lock(),
        scissor: *state.scissor.lock(),
        stencil_compare: StencilCompare::Disabled,
        stencil_ref: 0,
    };
    let job = crate::render_queue::RenderJob::with_coverage(
        coverage,
        move |buffer: &mut PixelBuffer, hidden| {
            let started = PROFILE_ENABLED.then(Instant::now);
            draw_state.apply(buffer);
            crate::occlusion::draw_visible(buffer, hidden, &draw);
            if let Some(started) = started {
                DEFERRED_CALLS.fetch_add(1, Ordering::Relaxed);
                DEFERRED_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
        },
    );
    if let Err(job) = state.submit_render_job(job) {
        state.with_active_buffer(|buffer| job.run(buffer, None));
    }
    true
}

impl LuaUserData for QuadData {}

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let g = lua.create_table()?;
    let queued_output = std::env::var("BALATRO_FRAME_PIPELINE").as_deref() == Ok("2");

    // love.graphics.clear([r, g, b, a])
    {
        let s = Arc::clone(&state);
        g.set(
            "clear",
            lua.create_function(move |_, args: LuaMultiValue| {
                let _profile = ProfileTimer::start(&CLEAR_CALLS, &CLEAR_NS);
                let color = if args.is_empty() {
                    *s.background_color.lock()
                } else {
                    parse_color(&args)
                };
                if queued_output
                    && defer_screen_draw(&s, move |pb| {
                        pb.clear(color[0], color[1], color[2], color[3]);
                        pb.clear_stencil();
                    })
                {
                    return Ok(());
                }
                s.with_active_buffer(|pb| {
                    pb.clear(color[0], color[1], color[2], color[3]);
                    pb.clear_stencil();
                });
                Ok(())
            })?,
        )?;
    }

    // love.graphics.setColor(r, g, b [, a]) or setColor({r, g, b, a})
    {
        let s = Arc::clone(&state);
        g.set(
            "setColor",
            lua.create_function(move |_, args: LuaMultiValue| {
                let c = parse_color(&args);
                *s.current_color.lock() = c;
                Ok(())
            })?,
        )?;
    }

    // love.graphics.getColor()
    {
        let s = Arc::clone(&state);
        g.set(
            "getColor",
            lua.create_function(move |_, ()| {
                let c = *s.current_color.lock();
                Ok((c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64))
            })?,
        )?;
    }

    // love.graphics.setBackgroundColor(r, g, b [, a])
    {
        let s = Arc::clone(&state);
        g.set(
            "setBackgroundColor",
            lua.create_function(move |_, args: LuaMultiValue| {
                *s.background_color.lock() = parse_color(&args);
                Ok(())
            })?,
        )?;
    }

    // love.graphics.rectangle(mode, x, y, w, h [, rx, ry])
    {
        let s = Arc::clone(&state);
        g.set(
            "rectangle",
            lua.create_function(
                move |_,
                      (mode, x, y, w, h, rx, ry): (
                    String,
                    f32,
                    f32,
                    f32,
                    f32,
                    Option<f32>,
                    Option<f32>,
                )| {
                    let _profile = ProfileTimer::start(&RECT_CALLS, &RECT_NS);
                    let color = color_f32_to_u8(*s.current_color.lock());
                    let t = current_transform(&s);

                    // Detect rotation in transform (b/c non-zero means shear/rotation)
                    let has_rotation = t.b.abs() > 0.001 || t.c.abs() > 0.001;

                    if has_rotation && mode == "fill" {
                        // Rotated rectangle: transform all 4 corners and fill as polygon
                        let corners = [
                            t.apply(x, y),
                            t.apply(x + w, y),
                            t.apply(x + w, y + h),
                            t.apply(x, y + h),
                        ];
                        if !defer_screen_draw(&s, move |pb| {
                            fill_polygon(pb, &corners, color);
                        }) {
                            s.with_active_buffer(|pb| fill_polygon(pb, &corners, color));
                        }
                        return Ok(());
                    }

                    let (p0x, p0y) = t.apply(x, y);
                    let (p1x, p1y) = t.apply(x + w, y + h);
                    let px = p0x.min(p1x) as i32;
                    let py = p0y.min(p1y) as i32;
                    let pw = (p1x - p0x).abs() as i32;
                    let ph = (p1y - p0y).abs() as i32;

                    let (scale_x, scale_y) = t.scale_factor();
                    let lw = ((*s.line_width.lock()) * scale_x.max(scale_y)).max(1.0) as u32;
                    let rrx = (rx.unwrap_or(0.0) * scale_x) as i32;
                    let rry = (ry.unwrap_or_else(|| rx.unwrap_or(0.0)) * scale_y) as i32;

                    let mode_is_fill = mode == "fill";
                    if defer_screen_draw(&s, move |pb| {
                        if mode_is_fill {
                            if rrx > 0 || rry > 0 {
                                pb.fill_rounded_rect(px, py, pw, ph, rrx, rry, color);
                            } else {
                                pb.fill_rect(px, py, pw, ph, color);
                            }
                        } else if rrx > 0 || rry > 0 {
                            pb.stroke_rounded_rect(px, py, pw, ph, rrx, rry, lw, color);
                        } else {
                            pb.stroke_rect(px, py, pw, ph, lw, color);
                        }
                    }) {
                        return Ok(());
                    }
                    s.with_active_buffer(|pb| {
                        if mode == "fill" {
                            if rrx > 0 || rry > 0 {
                                pb.fill_rounded_rect(px, py, pw, ph, rrx, rry, color);
                            } else {
                                pb.fill_rect(px, py, pw, ph, color);
                            }
                        } else if rrx > 0 || rry > 0 {
                            pb.stroke_rounded_rect(px, py, pw, ph, rrx, rry, lw, color);
                        } else {
                            pb.stroke_rect(px, py, pw, ph, lw, color);
                        }
                    });
                    Ok(())
                },
            )?,
        )?;
    }

    // love.graphics.print — no-op in TUI mode.
    // Balatro renders ALL game text via SpriteBatch, not print/printf.
    // Making these no-ops removes debug text (FPS counter, version numbers).
    {
        g.set(
            "print",
            lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
        )?;
    }

    // love.graphics.printf — no-op (same reason as print)
    {
        g.set(
            "printf",
            lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
        )?;
    }

    // love.graphics.push()
    {
        let s = Arc::clone(&state);
        g.set(
            "push",
            lua.create_function(move |_, args: LuaMultiValue| {
                let is_all = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy() == "all",
                    _ => false,
                };
                let mut stack = s.transform_stack.lock();
                let top = stack.last().cloned().unwrap_or_default();
                stack.push(top);
                drop(stack);
                if is_all {
                    s.state_stack.lock().push(Some(SavedGraphicsState {
                        color: *s.current_color.lock(),
                        scissor: *s.scissor.lock(),
                        stencil_compare: *s.stencil_compare.lock(),
                        stencil_ref: *s.stencil_ref.lock(),
                        line_width: *s.line_width.lock(),
                        font_size: *s.active_font_size.lock(),
                        font_id: *s.active_font_id.lock(),
                        active_canvas: *s.active_canvas.lock(),
                        blend_mode: *s.blend_mode.lock(),
                    }));
                } else {
                    s.state_stack.lock().push(None);
                }
                Ok(())
            })?,
        )?;
    }

    // Balatro prepares every sprite with the same transform sequence. Accepting
    // precomputed scalars keeps this hot path to one Lua callback and one lock.
    {
        let s = Arc::clone(&state);
        g.set(
            "_pushTransform",
            lua.create_function(
                move |_,
                      (world_scale, x, y, angle, origin_x, origin_y, object_scale): (
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                )| {
                    let mut stack = s.transform_stack.lock();
                    let transform = prepare_draw_transform(
                        stack.last().cloned().unwrap_or_default(),
                        world_scale,
                        x,
                        y,
                        angle,
                        origin_x,
                        origin_y,
                        object_scale,
                    );
                    stack.push(transform);
                    drop(stack);
                    s.state_stack.lock().push(None);
                    Ok(())
                },
            )?,
        )?;
    }

    // love.graphics.pop()
    {
        let s = Arc::clone(&state);
        g.set(
            "pop",
            lua.create_function(move |_, ()| {
                let mut stack = s.transform_stack.lock();
                if stack.len() > 1 {
                    stack.pop();
                }
                drop(stack);
                if let Some(saved_opt) = s.state_stack.lock().pop() {
                    if let Some(saved) = saved_opt {
                        *s.current_color.lock() = saved.color;
                        *s.scissor.lock() = saved.scissor;
                        *s.stencil_compare.lock() = saved.stencil_compare;
                        *s.stencil_ref.lock() = saved.stencil_ref;
                        *s.line_width.lock() = saved.line_width;
                        *s.active_font_size.lock() = saved.font_size;
                        *s.active_font_id.lock() = saved.font_id;
                        // Restore canvas first so subsequent syncs target the right buffer
                        *s.active_canvas.lock() = saved.active_canvas;
                        *s.blend_mode.lock() = saved.blend_mode;
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.translate(x, y)
    {
        let s = Arc::clone(&state);
        g.set(
            "translate",
            lua.create_function(move |_, (x, y): (f32, f32)| {
                let mut stack = s.transform_stack.lock();
                if let Some(t) = stack.last_mut() {
                    t.translate(x, y);
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.scale(sx [, sy])
    {
        let s = Arc::clone(&state);
        g.set(
            "scale",
            lua.create_function(move |_, (sx, sy): (f32, Option<f32>)| {
                let sy = sy.unwrap_or(sx);
                let mut stack = s.transform_stack.lock();
                if let Some(t) = stack.last_mut() {
                    t.scale(sx, sy);
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.rotate(r)
    {
        let s = Arc::clone(&state);
        g.set(
            "rotate",
            lua.create_function(move |_, r: f32| {
                let mut stack = s.transform_stack.lock();
                if let Some(t) = stack.last_mut() {
                    t.rotate(r);
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.origin()
    {
        let s = Arc::clone(&state);
        g.set(
            "origin",
            lua.create_function(move |_, ()| {
                let mut stack = s.transform_stack.lock();
                if let Some(t) = stack.last_mut() {
                    *t = Transform::default();
                }
                Ok(())
            })?,
        )?;
    }

    // love.graphics.reset()
    {
        let s = Arc::clone(&state);
        g.set(
            "reset",
            lua.create_function(move |_, ()| {
                *s.transform_stack.lock() = vec![Transform::default()];
                *s.current_color.lock() = [1.0, 1.0, 1.0, 1.0];
                *s.line_width.lock() = 1.0;
                *s.active_font_size.lock() = 12.0;
                *s.active_font_id.lock() = 0;
                *s.scissor.lock() = None;
                *s.stencil_compare.lock() = StencilCompare::Disabled;
                *s.stencil_ref.lock() = 0;
                *s.active_canvas.lock() = 0;
                *s.blend_mode.lock() = BlendMode::default();
                Ok(())
            })?,
        )?;
    }

    // love.graphics.getWidth()
    {
        let s = Arc::clone(&state);
        g.set(
            "getWidth",
            lua.create_function(move |_, ()| Ok(*s.canvas_width.lock()))?,
        )?;
    }

    // love.graphics.getHeight()
    {
        let s = Arc::clone(&state);
        g.set(
            "getHeight",
            lua.create_function(move |_, ()| Ok(*s.canvas_height.lock()))?,
        )?;
    }

    // love.graphics.getDimensions()
    {
        let s = Arc::clone(&state);
        g.set(
            "getDimensions",
            lua.create_function(move |_, ()| {
                Ok((*s.canvas_width.lock(), *s.canvas_height.lock()))
            })?,
        )?;
    }

    // love.graphics.getPixelWidth() — alias for getWidth
    {
        let s = Arc::clone(&state);
        g.set(
            "getPixelWidth",
            lua.create_function(move |_, ()| Ok(*s.canvas_width.lock()))?,
        )?;
    }

    // love.graphics.getPixelHeight() — alias for getHeight
    {
        let s = Arc::clone(&state);
        g.set(
            "getPixelHeight",
            lua.create_function(move |_, ()| Ok(*s.canvas_height.lock()))?,
        )?;
    }

    // love.graphics.isActive()
    g.set("isActive", lua.create_function(|_, ()| Ok(true))?)?;

    // love.graphics.isCreated()
    g.set("isCreated", lua.create_function(|_, ()| Ok(true))?)?;

    // love.graphics.present() — noop, rendering happens in Rust main loop
    g.set("present", lua.create_function(|_, ()| Ok(()))?)?;

    // love.graphics.setLineWidth(width)
    {
        let s = Arc::clone(&state);
        g.set(
            "setLineWidth",
            lua.create_function(move |_, w: f32| {
                *s.line_width.lock() = w;
                Ok(())
            })?,
        )?;
    }

    // love.graphics.getLineWidth()
    {
        let s = Arc::clone(&state);
        g.set(
            "getLineWidth",
            lua.create_function(move |_, ()| Ok(*s.line_width.lock()))?,
        )?;
    }

    draw::register(lua, &g, &state, queued_output)?;

    // love.graphics.line(x1, y1, x2, y2, ...)
    {
        let s = Arc::clone(&state);
        g.set(
            "line",
            lua.create_function(move |_, args: LuaMultiValue| {
                let _profile = ProfileTimer::start(&LINE_CALLS, &LINE_NS);
                // Collect all coordinates as f32
                let mut coords: Vec<f32> = Vec::with_capacity(args.len());
                for arg in args.iter() {
                    match arg {
                        LuaValue::Number(n) => coords.push(*n as f32),
                        LuaValue::Integer(n) => coords.push(*n as f32),
                        _ => break,
                    }
                }
                if coords.len() < 4 || coords.len() % 2 != 0 {
                    return Ok(());
                }

                let color = color_f32_to_u8(*s.current_color.lock());
                let t = current_transform(&s);
                let lw = *s.line_width.lock();
                let (sfx, _) = t.scale_factor();
                let scaled_lw = (lw * sfx).max(1.0);

                s.with_active_buffer(|pb| {
                    let mut i = 0;
                    while i + 3 < coords.len() {
                        let (x0, y0) = t.apply(coords[i], coords[i + 1]);
                        let (x1, y1) = t.apply(coords[i + 2], coords[i + 3]);
                        draw_thick_line(pb, x0, y0, x1, y1, scaled_lw, color);
                        i += 2;
                    }
                });
                Ok(())
            })?,
        )?;
    }

    // love.graphics.circle(mode, x, y, radius [, segments])
    {
        let s = Arc::clone(&state);
        g.set(
            "circle",
            lua.create_function(move |_, (mode, cx, cy, radius, _segments): (String, f32, f32, f32, Option<u32>)| {
                let _profile = ProfileTimer::start(&ELLIPSE_CALLS, &ELLIPSE_NS);
                let color = color_f32_to_u8(*s.current_color.lock());
                let t = current_transform(&s);
                let (px_f, py_f) = t.apply(cx, cy);
                let px = px_f as i32;
                let py = py_f as i32;
                let (sfx, sfy) = t.scale_factor();
                let rx = (radius * sfx) as i32;
                let ry = (radius * sfy) as i32;

                s.with_active_buffer(|pb| {
                    if mode == "fill" {
                        draw_filled_ellipse(pb, px, py, rx, ry, color);
                    } else {
                        draw_stroke_ellipse(pb, px, py, rx, ry, color);
                    }
                });
                Ok(())
            })?,
        )?;
    }

    // love.graphics.ellipse(mode, x, y, radiusx, radiusy [, segments])
    {
        let s = Arc::clone(&state);
        g.set(
            "ellipse",
            lua.create_function(move |_, (mode, cx, cy, rx, ry, _seg): (String, f32, f32, f32, f32, Option<u32>)| {
                let _profile = ProfileTimer::start(&ELLIPSE_CALLS, &ELLIPSE_NS);
                let color = color_f32_to_u8(*s.current_color.lock());
                let t = current_transform(&s);
                let (px, py) = t.apply(cx, cy);
                let (sfx, sfy) = t.scale_factor();
                let irx = (rx * sfx) as i32;
                let iry = (ry * sfy) as i32;
                s.with_active_buffer(|pb| {
                    if mode == "fill" {
                        draw_filled_ellipse(pb, px as i32, py as i32, irx, iry, color);
                    } else {
                        draw_stroke_ellipse(pb, px as i32, py as i32, irx, iry, color);
                    }
                });
                Ok(())
            })?,
        )?;
    }

    // love.graphics.arc(mode, x, y, radius, angle1, angle2 [, segments])
    {
        let s = Arc::clone(&state);
        g.set(
            "arc",
            lua.create_function(
                move |_,
                      (mode, cx, cy, radius, a1, a2, _seg): (
                    String,
                    f32,
                    f32,
                    f32,
                    f32,
                    f32,
                    Option<u32>,
                )| {
                    let _profile = ProfileTimer::start(&ELLIPSE_CALLS, &ELLIPSE_NS);
                    let color = color_f32_to_u8(*s.current_color.lock());
                    let t = current_transform(&s);
                    let (px, py) = t.apply(cx, cy);
                    let (sfx, sfy) = t.scale_factor();
                    let irx = (radius * sfx) as i32;
                    let iry = (radius * sfy) as i32;
                    // Approximate arc by drawing full ellipse (acceptable for terminal resolution)
                    s.with_active_buffer(|pb| {
                        if mode == "fill" {
                            draw_filled_ellipse(pb, px as i32, py as i32, irx, iry, color);
                        } else {
                            draw_stroke_ellipse(pb, px as i32, py as i32, irx, iry, color);
                        }
                    });
                    let _ = (a1, a2); // angles ignored in terminal approximation
                    Ok(())
                },
            )?,
        )?;
    }

    // love.graphics.points(...)
    {
        let s = Arc::clone(&state);
        g.set(
            "points",
            lua.create_function(move |_, args: LuaMultiValue| {
                let color = color_f32_to_u8(*s.current_color.lock());
                let t = current_transform(&s);
                let mut coords: Vec<f32> = Vec::new();
                for arg in args.iter() {
                    match arg {
                        LuaValue::Number(n) => coords.push(*n as f32),
                        LuaValue::Integer(n) => coords.push(*n as f32),
                        _ => break,
                    }
                }
                s.with_active_buffer(|pb| {
                    for pair in coords.chunks_exact(2) {
                        let (px, py) = t.apply(pair[0], pair[1]);
                        pb.set_pixel(px as u32, py as u32, color[0], color[1], color[2], color[3]);
                    }
                });
                Ok(())
            })?,
        )?;
    }

    images::register(lua, &g, &state)?;
    text::register(lua, &g, &state)?;
    canvas::register(lua, &g, &state)?;
    shaders::register(lua, &g, &state)?;
    sprite_batch::register(lua, &g, &state)?;
    state_bindings::register(lua, &g, &state)?;

    love.set("graphics", g)?;
    Ok(())
}

#[cfg(test)]
#[path = "graphics_transform_tests.rs"]
mod transform_tests;

/// Parse a numeric argument from a LuaValue, with a default fallback.
#[inline]
fn parse_num_arg(val: Option<&LuaValue>, default: f32) -> f32 {
    match val {
        Some(LuaValue::Number(n)) => *n as f32,
        Some(LuaValue::Integer(n)) => *n as f32,
        _ => default,
    }
}
