// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

mod card_shader;
mod effects;
mod image;

pub use crate::card_effects::CardShaderInputs;
pub(crate) use card_shader::apply_card_shader;
use card_shader::{apply_voucher_booster_axis, VoucherColumn, VoucherRow};
#[cfg(feature = "layer-pairs")]
#[path = "card_layer_pair.rs"]
mod card_layer_pair;
use crate::card_effects::{fast_cos, fast_sin, spatial, ShaderPre, TrigLookup};
use crate::dissolve::{DissolveCache, DissolveField};
use crate::image_sampling::average_box;
use crate::shader_batch::ShaderBatch;
use crate::shader_colour::{hsl_to_rgb, wrap_hue};
use crate::shader_colour_cache::{card_hsl, ColourCache};
use crate::shader_spatial_cache::SpatialCache;
#[cfg(feature = "shader-cache-stats")]
pub use crate::shader_spatial_cache::SpatialCacheStats;

static SHADER_COLOUR_CACHE_ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    std::env::var("BALATRO_SHADER_COLOUR_CACHE").is_ok_and(|value| value == "1")
});

static SHADER_SPATIAL_CACHE_ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    std::env::var("BALATRO_SHADER_SPATIAL_CACHE").is_ok_and(|value| value == "1")
});

/// Parameters for the dissolve shader emulation.
#[derive(Clone, Copy)]
pub struct DissolveParams {
    /// Dissolve amount: 0.0 = fully visible, 0.6+ = fully invisible
    pub dissolve: f32,
    /// Inner burn edge color [r,g,b,a] (0..255)
    pub burn1: [u8; 4],
    /// Outer burn edge color [r,g,b,a] (0..255)
    pub burn2: [u8; 4],
    /// Card shader effect: 0=none, 1=played, 2=debuff, 3=foil, 4=holo, 5=polychrome, 6=negative, 7=voucher, 8=booster, 9=hologram, 10=negative_shine, 11=gold_seal
    pub shader_effect: u8,
    /// Card phase, game clock and the separate dissolve/noise seed.
    pub shader_inputs: CardShaderInputs,
    /// Sprite pixel dimensions for dissolve noise field (from texture_details.ba)
    pub sprite_w: f32,
    pub sprite_h: f32,
}

impl DissolveParams {
    pub const NONE: Self = Self {
        dissolve: 0.0,
        burn1: [0, 0, 0, 0],
        burn2: [0, 0, 0, 0],
        shader_effect: 0,
        shader_inputs: CardShaderInputs {
            phase: 0.0,
            clock: 0.0,
            seed: 0.0,
        },
        sprite_w: 71.0,
        sprite_h: 95.0,
    };
}

/// GLSL-matching dissolve noise field from dissolve_mask() in Balatro's shaders.
/// Returns `res` value; pixel is visible when `res > adjusted_dissolve`.
#[inline(always)]
#[cfg(test)]
fn dissolve_field(
    ux: f32,
    uy: f32,
    time: f32,
    dissolve: f32,
    adjusted_dissolve: f32,
    sprite_w: f32,
    sprite_h: f32,
) -> f32 {
    let max_dim = sprite_w.max(sprite_h);
    let floored_x = (ux * sprite_w).floor() / max_dim;
    let floored_y = (uy * sprite_h).floor() / max_dim;
    let usc_x = (floored_x - 0.5) * 2.3 * max_dim;
    let usc_y = (floored_y - 0.5) * 2.3 * max_dim;

    let t = time * 10.0 + 2003.0;
    let f1x = usc_x + 50.0 * fast_sin(-t / 143.634);
    let f1y = usc_y + 50.0 * fast_cos(-t / 99.4324);
    let f2x = usc_x + 50.0 * fast_cos(t / 53.1532);
    let f2y = usc_y + 50.0 * fast_cos(t / 61.4532);
    let f3x = usc_x + 50.0 * fast_sin(-t / 87.53218);
    let f3y = usc_y + 50.0 * fast_sin(-t / 49.0);

    let len1 = (f1x * f1x + f1y * f1y).sqrt();
    let len2 = (f2x * f2x + f2y * f2y).sqrt();
    let len3 = (f3x * f3x + f3y * f3y).sqrt();

    let field = (1.0
        + fast_cos(len1 / 19.483)
        + fast_sin(len2 / 33.155) * fast_cos(f2y / 15.73)
        + fast_cos(len3 / 27.193) * fast_sin(f3x / 21.92))
        / 2.0;

    let d = dissolve;
    0.5 + 0.5 * fast_cos(adjusted_dissolve / 82.612 + (field - 0.5) * std::f32::consts::PI)
        - if floored_x > 0.8 {
            (floored_x - 0.8) * (5.0 + 5.0 * d) * d
        } else {
            0.0
        }
        - if floored_y > 0.8 {
            (floored_y - 0.8) * (5.0 + 5.0 * d) * d
        } else {
            0.0
        }
        - if floored_x < 0.2 {
            (0.2 - floored_x) * (5.0 + 5.0 * d) * d
        } else {
            0.0
        }
        - if floored_y < 0.2 {
            (0.2 - floored_y) * (5.0 + 5.0 * d) * d
        } else {
            0.0
        }
}

/// Stencil compare mode for `setStencilTest(compare, value)`.
#[derive(Clone, Copy, PartialEq)]
pub enum StencilCompare {
    Disabled,
    Greater,  // pixel drawn only where stencil > value
    GEqual,   // pixel drawn only where stencil >= value
    Equal,    // pixel drawn only where stencil == value
    LEqual,   // pixel drawn only where stencil <= value
    Less,     // pixel drawn only where stencil < value
    NotEqual, // pixel drawn only where stencil != value
    Always,   // always draw (ignore stencil)
    Never,    // never draw
}

#[inline(always)]
pub(crate) fn blend_source_over_pixel(pixel: &mut [u8], color: [u8; 4]) {
    let alpha = color[3] as u16;
    if alpha == 0 {
        return;
    }
    let inverse_alpha = 255 - alpha;
    let destination_alpha = pixel[3] as u16;
    pixel[0] = ((color[0] as u16 * alpha + pixel[0] as u16 * inverse_alpha) / 255) as u8;
    pixel[1] = ((color[1] as u16 * alpha + pixel[1] as u16 * inverse_alpha) / 255) as u8;
    pixel[2] = ((color[2] as u16 * alpha + pixel[2] as u16 * inverse_alpha) / 255) as u8;
    pixel[3] = (alpha + destination_alpha * inverse_alpha / 255).min(255) as u8;
}

/// RGBA pixel buffer for the game's coordinate space, stored row-major.
pub struct PixelBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA, 4 bytes per pixel
    /// Optional scissor rectangle (x, y, w, h) for clipping
    pub scissor: Option<(i32, i32, u32, u32)>,
    /// Stencil buffer — one byte per pixel, same w*h as pixels
    pub stencil: Vec<u8>,
    /// When true, drawing operations write to stencil instead of pixels
    pub stencil_write_mode: bool,
    /// Stencil compare function + reference value
    pub stencil_compare: StencilCompare,
    pub stencil_ref: u8,
    /// Blend mode: 0=alpha (default), 1=replace, 2=add, 3=multiply, 4=premultiplied
    pub blend: u8,
    /// Filter mode: false=nearest (default), true=linear (bilinear interpolation)
    pub filter_linear: bool,
    // Reusable CRT bloom buffers (avoid per-frame allocation)
    crt_bright: Vec<u16>,
    crt_temp: Vec<u16>,
    crt_col_bloom: Vec<(usize, usize, u32)>,
    nearest_columns: Vec<usize>,
    voucher_columns: Vec<VoucherColumn>,
    pub(crate) shader_colour_cache: Option<Box<ColourCache>>,
    shader_colour_cache_enabled: bool,
    shader_spatial_cache: SpatialCache,
    shader_spatial_cache_enabled: bool,
    dissolve_cache: DissolveCache,
}

impl PixelBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        PixelBuffer {
            width,
            height,
            pixels: vec![0u8; (width * height * 4) as usize],
            scissor: None,
            stencil: vec![0u8; (width * height) as usize],
            stencil_write_mode: false,
            stencil_compare: StencilCompare::Disabled,
            stencil_ref: 0,
            blend: 0,
            filter_linear: false,
            crt_bright: Vec::new(),
            crt_temp: Vec::new(),
            crt_col_bloom: Vec::new(),
            nearest_columns: Vec::new(),
            voucher_columns: Vec::new(),
            shader_colour_cache: None,
            shader_colour_cache_enabled: *SHADER_COLOUR_CACHE_ENABLED,
            shader_spatial_cache: SpatialCache::default(),
            shader_spatial_cache_enabled: *SHADER_SPATIAL_CACHE_ENABLED,
            dissolve_cache: DissolveCache::default(),
        }
    }

    pub fn copy_raster_source(&mut self, source: &Self) {
        if (self.width, self.height) != (source.width, source.height) {
            self.resize(source.width, source.height);
        }
        self.pixels.copy_from_slice(&source.pixels);
        self.stencil.copy_from_slice(&source.stencil);
        self.scissor = source.scissor;
        self.blend = source.blend;
        self.filter_linear = source.filter_linear;
        self.stencil_write_mode = source.stencil_write_mode;
        self.stencil_compare = source.stencil_compare;
        self.stencil_ref = source.stencil_ref;
        self.set_shader_colour_cache(source.shader_colour_cache_enabled);
        self.set_shader_spatial_cache(source.shader_spatial_cache_enabled);
    }

    pub fn set_shader_colour_cache(&mut self, enabled: bool) {
        self.shader_colour_cache_enabled = enabled;
        if !enabled {
            self.shader_colour_cache = None;
        }
    }

    pub fn set_shader_spatial_cache(&mut self, enabled: bool) {
        self.shader_spatial_cache_enabled = enabled;
        if !enabled {
            self.shader_spatial_cache = SpatialCache::default();
        }
    }

    fn prepare_shader_colour_cache(&mut self, effect: u8) {
        if self.shader_colour_cache_enabled && matches!(effect, 1 | 2 | 4 | 5 | 6) {
            self.shader_colour_cache
                .get_or_insert_with(|| Box::new(ColourCache::new()));
        }
    }

    #[cfg(feature = "shader-cache-stats")]
    pub fn shader_colour_cache_stats(&self) -> (u64, u64) {
        self.shader_colour_cache
            .as_ref()
            .map_or((0, 0), |cache| cache.stats())
    }

    #[cfg(feature = "shader-cache-stats")]
    pub fn shader_spatial_cache_stats(&self) -> SpatialCacheStats {
        self.shader_spatial_cache.stats()
    }

    /// Resize the buffer to new dimensions, clearing all pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let size = (width * height) as usize;
        self.pixels.resize(size * 4, 0);
        self.pixels.fill(0);
        self.stencil.resize(size, 0);
        self.stencil.fill(0);
        self.scissor = None;
    }

    /// Clear stencil buffer to zero.
    pub fn clear_stencil(&mut self) {
        for v in self.stencil.iter_mut() {
            *v = 0;
        }
    }

    /// Test stencil at pixel position. Returns true if drawing is allowed.
    #[inline(always)]
    pub(crate) fn stencil_test(&self, x: u32, y: u32) -> bool {
        match self.stencil_compare {
            StencilCompare::Disabled | StencilCompare::Always => true,
            StencilCompare::Never => false,
            _ => {
                let si = (y * self.width + x) as usize;
                let sv = if si < self.stencil.len() {
                    self.stencil[si]
                } else {
                    0
                };
                let rv = self.stencil_ref;
                match self.stencil_compare {
                    StencilCompare::Greater => sv > rv,
                    StencilCompare::GEqual => sv >= rv,
                    StencilCompare::Equal => sv == rv,
                    StencilCompare::LEqual => sv <= rv,
                    StencilCompare::Less => sv < rv,
                    StencilCompare::NotEqual => sv != rv,
                    _ => true,
                }
            }
        }
    }

    /// Write to stencil buffer at pixel position (increment by 1, capped at 255).
    #[inline(always)]
    fn stencil_write(&mut self, x: u32, y: u32) {
        let si = (y * self.width + x) as usize;
        if si < self.stencil.len() {
            self.stencil[si] = self.stencil[si].saturating_add(1);
        }
    }

    pub fn clear(&mut self, r: f32, g: f32, b: f32, a: f32) {
        let ri = (r.clamp(0.0, 1.0) * 255.0) as u8;
        let gi = (g.clamp(0.0, 1.0) * 255.0) as u8;
        let bi = (b.clamp(0.0, 1.0) * 255.0) as u8;
        let ai = (a.clamp(0.0, 1.0) * 255.0) as u8;
        static BULK_CLEAR: std::sync::LazyLock<bool> =
            std::sync::LazyLock::new(|| std::env::var("BALATRO_BULK_CLEAR").as_deref() != Ok("0"));
        if *BULK_CLEAR {
            for pixel in self.pixels.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[ri, gi, bi, ai]);
            }
            return;
        }
        let total = self.pixels.len();
        if total < 4 {
            return;
        }
        // Seed first pixel
        self.pixels[0] = ri;
        self.pixels[1] = gi;
        self.pixels[2] = bi;
        self.pixels[3] = ai;
        // Doubling copy: fill 4, 8, 16, ... bytes at a time
        let mut filled = 4;
        while filled < total {
            let copy_len = filled.min(total - filled);
            self.pixels.copy_within(0..copy_len, filled);
            filled += copy_len;
        }
    }

    /// Compute clipping bounds as (x0, y0, x1, y1) — intersection of buffer and scissor.
    #[inline]
    fn clip_bounds(&self) -> (i32, i32, i32, i32) {
        if let Some((sx, sy, sw, sh)) = self.scissor {
            (
                sx.max(0),
                sy.max(0),
                (sx + sw as i32).min(self.width as i32),
                (sy + sh as i32).min(self.height as i32),
            )
        } else {
            (0, 0, self.width as i32, self.height as i32)
        }
    }

    /// Blend a pixel at a known-valid index, dispatching by active blend mode.
    /// Modes: 0=alpha (source-over), 1=replace, 2=add, 3=multiply, 4=premultiplied, 5=screen
    #[inline(always)]
    pub fn blend_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        match self.blend {
            2 => self.blend_add_at(idx, r, g, b, a),
            3 => self.blend_multiply_at(idx, r, g, b, a),
            4 => self.blend_premultiplied_at(idx, r, g, b, a),
            5 => self.blend_screen_at(idx, r, g, b, a),
            _ => {
                // Default: source-over alpha compositing
                if a == 255 {
                    self.pixels[idx] = r;
                    self.pixels[idx + 1] = g;
                    self.pixels[idx + 2] = b;
                    self.pixels[idx + 3] = 255;
                } else if a > 0 {
                    let sa = a as u16;
                    let da = self.pixels[idx + 3] as u16;
                    let inv_sa = 255 - sa;
                    let out_a = sa + (da * inv_sa / 255);
                    self.pixels[idx] =
                        ((r as u16 * sa + self.pixels[idx] as u16 * inv_sa) / 255) as u8;
                    self.pixels[idx + 1] =
                        ((g as u16 * sa + self.pixels[idx + 1] as u16 * inv_sa) / 255) as u8;
                    self.pixels[idx + 2] =
                        ((b as u16 * sa + self.pixels[idx + 2] as u16 * inv_sa) / 255) as u8;
                    self.pixels[idx + 3] = out_a.min(255) as u8;
                }
            }
        }
    }

    #[inline]
    fn blend_solid_span(&mut self, start: usize, pixel_count: usize, color: [u8; 4]) {
        let end = start + pixel_count * 4;
        let pixels = &mut self.pixels[start..end];

        #[cfg(all(target_arch = "arm", feature = "arm-neon"))]
        unsafe {
            crate::neon::blend_solid_span(pixels, color);
            return;
        }

        #[cfg(not(all(target_arch = "arm", feature = "arm-neon")))]
        for pixel in pixels.chunks_exact_mut(4) {
            blend_source_over_pixel(pixel, color);
        }
    }

    /// Additive blend: dst += src * alpha (clamped at 255).
    #[inline(always)]
    fn blend_add_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        if a == 0 {
            return;
        }
        let sa = a as u16;
        self.pixels[idx] = (self.pixels[idx] as u16 + r as u16 * sa / 255).min(255) as u8;
        self.pixels[idx + 1] = (self.pixels[idx + 1] as u16 + g as u16 * sa / 255).min(255) as u8;
        self.pixels[idx + 2] = (self.pixels[idx + 2] as u16 + b as u16 * sa / 255).min(255) as u8;
        // Alpha: keep existing or saturate
        self.pixels[idx + 3] = (self.pixels[idx + 3] as u16 + sa).min(255) as u8;
    }

    /// Multiply blend: dst *= src (component-wise, src alpha-premultiplied).
    #[inline(always)]
    fn blend_multiply_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        if a == 0 {
            return;
        }
        let sa = a as u32;
        let inv_sa = 255 - sa;
        // multiply = dst * src * alpha + dst * (1 - alpha)
        self.pixels[idx] = (self.pixels[idx] as u32 * r as u32 * sa / 65025
            + self.pixels[idx] as u32 * inv_sa / 255)
            .min(255) as u8;
        self.pixels[idx + 1] = (self.pixels[idx + 1] as u32 * g as u32 * sa / 65025
            + self.pixels[idx + 1] as u32 * inv_sa / 255)
            .min(255) as u8;
        self.pixels[idx + 2] = (self.pixels[idx + 2] as u32 * b as u32 * sa / 65025
            + self.pixels[idx + 2] as u32 * inv_sa / 255)
            .min(255) as u8;
    }

    /// Premultiplied alpha blend: source RGB already multiplied by alpha.
    /// Formula: dst = src + dst * (1 - src_a)
    #[inline(always)]
    fn blend_premultiplied_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        if a == 255 {
            self.pixels[idx] = r;
            self.pixels[idx + 1] = g;
            self.pixels[idx + 2] = b;
            self.pixels[idx + 3] = 255;
        } else if a > 0 {
            let inv_sa = (255 - a) as u16;
            self.pixels[idx] = (r as u16 + self.pixels[idx] as u16 * inv_sa / 255).min(255) as u8;
            self.pixels[idx + 1] =
                (g as u16 + self.pixels[idx + 1] as u16 * inv_sa / 255).min(255) as u8;
            self.pixels[idx + 2] =
                (b as u16 + self.pixels[idx + 2] as u16 * inv_sa / 255).min(255) as u8;
            self.pixels[idx + 3] =
                (a as u16 + self.pixels[idx + 3] as u16 * inv_sa / 255).min(255) as u8;
        }
    }

    /// Composite an unscaled premultiplied RGBA image and ignore its transparent border.
    pub fn composite_premultiplied_identity(
        &mut self,
        src_pixels: &[u8],
        src_w: u32,
        src_h: u32,
        dst_x: i32,
        dst_y: i32,
    ) {
        let required = src_w as usize * src_h as usize * 4;
        if src_w == 0 || src_h == 0 || src_pixels.len() < required {
            return;
        }

        let mut min_x = src_w;
        let mut min_y = src_h;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found = false;
        let stride = src_w as usize * 4;

        for y in 0..src_h {
            let row = &src_pixels[y as usize * stride..(y as usize + 1) * stride];
            let mut first = src_w;
            let mut last = 0;
            for x in 0..src_w {
                if row[x as usize * 4 + 3] != 0 {
                    first = first.min(x);
                    last = x + 1;
                }
            }
            if first < last {
                found = true;
                min_x = min_x.min(first);
                min_y = min_y.min(y);
                max_x = max_x.max(last);
                max_y = y + 1;
            }
        }
        if !found {
            return;
        }

        let (clip_x0, clip_y0, clip_x1, clip_y1) = self.clip_bounds();
        let src_x0 = min_x.max((clip_x0 - dst_x).max(0) as u32);
        let src_y0 = min_y.max((clip_y0 - dst_y).max(0) as u32);
        let src_x1 = max_x.min((clip_x1 - dst_x).max(0) as u32);
        let src_y1 = max_y.min((clip_y1 - dst_y).max(0) as u32);
        if src_x0 >= src_x1 || src_y0 >= src_y1 {
            return;
        }

        let dst_stride = self.width as usize * 4;
        for src_y in src_y0..src_y1 {
            let src_row = src_y as usize * stride;
            let dst_row = (dst_y + src_y as i32) as usize * dst_stride;
            for src_x in src_x0..src_x1 {
                let src = src_row + src_x as usize * 4;
                let alpha = src_pixels[src + 3];
                if alpha == 0 {
                    continue;
                }
                let dst = dst_row + (dst_x + src_x as i32) as usize * 4;
                self.blend_premultiplied_at(
                    dst,
                    src_pixels[src],
                    src_pixels[src + 1],
                    src_pixels[src + 2],
                    alpha,
                );
            }
        }
    }

    /// Screen blend: result = src + dst - src * dst (lightens image).
    /// With alpha: lerp between dst and screen(dst, src) by alpha.
    #[inline(always)]
    fn blend_screen_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        if a == 0 {
            return;
        }
        let sa = a as u16;
        let inv_sa = 255 - sa;
        // screen(s,d) = s + d - s*d/255
        // final = screen * alpha + dst * (1-alpha)
        let dr = self.pixels[idx] as u16;
        let dg = self.pixels[idx + 1] as u16;
        let db = self.pixels[idx + 2] as u16;
        let sr = r as u16 + dr - (r as u16 * dr / 255);
        let sg = g as u16 + dg - (g as u16 * dg / 255);
        let sb = b as u16 + db - (b as u16 * db / 255);
        self.pixels[idx] = ((sr * sa + dr * inv_sa) / 255).min(255) as u8;
        self.pixels[idx + 1] = ((sg * sa + dg * inv_sa) / 255).min(255) as u8;
        self.pixels[idx + 2] = ((sb * sa + db * inv_sa) / 255).min(255) as u8;
    }

    /// Write a pixel directly without blending (replace mode).
    #[inline(always)]
    pub fn write_at(&mut self, idx: usize, r: u8, g: u8, b: u8, a: u8) {
        self.pixels[idx] = r;
        self.pixels[idx + 1] = g;
        self.pixels[idx + 2] = b;
        self.pixels[idx + 3] = a;
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some((sx, sy, sw, sh)) = self.scissor {
            let xi = x as i32;
            let yi = y as i32;
            if xi < sx || xi >= sx + sw as i32 || yi < sy || yi >= sy + sh as i32 {
                return;
            }
        }
        // Stencil write mode: write to stencil buffer instead of pixels
        if self.stencil_write_mode {
            self.stencil_write(x, y);
            return;
        }
        // Stencil test: skip pixel if stencil test fails
        if self.stencil_compare != StencilCompare::Disabled && !self.stencil_test(x, y) {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.blend_at(idx, r, g, b, a);
    }

    /// Write a pixel directly without blending (for replace blend mode)
    #[inline(always)]
    pub fn set_pixel_replace(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some((sx, sy, sw, sh)) = self.scissor {
            let xi = x as i32;
            let yi = y as i32;
            if xi < sx || xi >= sx + sw as i32 || yi < sy || yi >= sy + sh as i32 {
                return;
            }
        }
        if self.stencil_write_mode {
            self.stencil_write(x, y);
            return;
        }
        if self.stencil_compare != StencilCompare::Disabled && !self.stencil_test(x, y) {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.write_at(idx, r, g, b, a);
    }

    /// Draw a filled axis-aligned rectangle
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        let (clip_x0, clip_y0, clip_x1, clip_y1) = self.clip_bounds();
        let x0 = x.max(clip_x0);
        let y0 = y.max(clip_y0);
        let x1 = (x + w).min(clip_x1);
        let y1 = (y + h).min(clip_y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let x0 = x0 as u32;
        let y0 = y0 as u32;
        let x1 = x1 as u32;
        let y1 = y1 as u32;
        let row_width = (x1 - x0) as usize;

        // Stencil write mode: mark stencil buffer instead of drawing pixels
        if self.stencil_write_mode {
            for py in y0..y1 {
                for px in x0..x1 {
                    self.stencil_write(px, py);
                }
            }
            return;
        }

        let use_stencil = self.stencil_compare != StencilCompare::Disabled;

        if color[3] == 255 && !use_stencil && self.blend == 0 {
            static ROW_FILL: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
                std::env::var("BALATRO_ROW_FILL").as_deref() != Ok("0")
            });
            let first_row_start = (y0 * self.width + x0) as usize * 4;
            let row_bytes = row_width * 4;
            if *ROW_FILL {
                for pixel in
                    self.pixels[first_row_start..first_row_start + row_bytes].chunks_exact_mut(4)
                {
                    pixel.copy_from_slice(&color);
                }
            } else {
                for i in 0..row_width {
                    let idx = first_row_start + i * 4;
                    self.pixels[idx] = color[0];
                    self.pixels[idx + 1] = color[1];
                    self.pixels[idx + 2] = color[2];
                    self.pixels[idx + 3] = 255;
                }
            }
            for py in (y0 + 1)..y1 {
                let dst_start = (py * self.width + x0) as usize * 4;
                self.pixels
                    .copy_within(first_row_start..first_row_start + row_bytes, dst_start);
            }
        } else if !use_stencil && self.blend == 0 {
            let buf_w = self.width;
            for py in y0..y1 {
                let row_base = (py * buf_w + x0) as usize * 4;
                self.blend_solid_span(row_base, row_width, color);
            }
        } else {
            // Per-pixel path (alpha blending and/or stencil test)
            let buf_w = self.width;
            for py in y0..y1 {
                let row_base = (py * buf_w + x0) as usize * 4;
                for i in 0..row_width {
                    if use_stencil && !self.stencil_test(x0 + i as u32, py) {
                        continue;
                    }
                    self.blend_at(row_base + i * 4, color[0], color[1], color[2], color[3]);
                }
            }
        }
    }

    /// Draw a stroked axis-aligned rectangle
    pub fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, line_width: u32, color: [u8; 4]) {
        let lw = line_width as i32;
        // Top edge
        self.fill_rect(x, y, w, lw, color);
        // Bottom edge
        self.fill_rect(x, y + h - lw, w, lw, color);
        // Left edge
        self.fill_rect(x, y + lw, lw, h - lw * 2, color);
        // Right edge
        self.fill_rect(x + w - lw, y + lw, lw, h - lw * 2, color);
    }

    pub(crate) fn flush_shader_batch(&mut self, batch: &mut ShaderBatch, replace: bool) {
        batch.shade();
        for n in 0..batch.len {
            let di = batch.destinations[n];
            let [r, g, b, a] = batch.color(n);
            if replace || ((self.blend == 0 || self.blend == 4) && a == 255) {
                self.pixels[di..di + 4].copy_from_slice(&[r, g, b, a]);
            } else if a != 0 {
                self.blend_at(di, r, g, b, a);
            }
        }
        batch.len = 0;
    }

    /// Save buffer as PPM image file (for debug screenshots)
    pub fn save_ppm(&self, path: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{} {}\n255\n", self.width, self.height)?;
        let mut rgb = Vec::with_capacity((self.width * self.height * 3) as usize);
        for chunk in self.pixels.chunks_exact(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }
        f.write_all(&rgb)?;
        Ok(())
    }

    /// Compute smooth coverage (0.0..1.0) for a pixel in a rounded rectangle.
    /// Returns 1.0 for fully inside, 0.0 for fully outside, fractional at edges.
    #[inline]
    fn rounded_rect_coverage(
        px: i32,
        py: i32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        rx: i32,
        ry: i32,
    ) -> f32 {
        if px < x || px >= x + w || py < y || py >= y + h {
            return 0.0;
        }
        // Determine if we're in a corner region
        let (dx, dy) = if px < x + rx && py < y + ry {
            (px - (x + rx), py - (y + ry))
        } else if px >= x + w - rx && py < y + ry {
            (px - (x + w - rx - 1), py - (y + ry))
        } else if px < x + rx && py >= y + h - ry {
            (px - (x + rx), py - (y + h - ry - 1))
        } else if px >= x + w - rx && py >= y + h - ry {
            (px - (x + w - rx - 1), py - (y + h - ry - 1))
        } else {
            return 1.0; // Not in a corner region — fully inside
        };
        // Ellipse distance: d = (dx/rx)^2 + (dy/ry)^2
        // d < 1.0 = inside, d > 1.0 = outside
        let rx_f = rx as f32;
        let ry_f = ry as f32;
        let nx = dx as f32 / rx_f;
        let ny = dy as f32 / ry_f;
        let d = nx * nx + ny * ny;
        if d <= 0.8 {
            1.0 // Well inside — skip smoothing
        } else if d >= 1.2 {
            0.0 // Well outside
        } else {
            // Smooth transition zone: map [0.8, 1.2] to [1.0, 0.0]
            // Uses smoothstep-like falloff for better anti-aliasing
            let t = (d - 0.8) * 2.5; // maps 0.8..1.2 → 0.0..1.0
            let t = t.clamp(0.0, 1.0);
            1.0 - t * t * (3.0 - 2.0 * t) // smoothstep
        }
    }

    /// Draw a filled rounded rectangle with anti-aliased corners
    pub fn fill_rounded_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        rx: i32,
        ry: i32,
        color: [u8; 4],
    ) {
        let rx = rx.min(w / 2).max(0);
        let ry = ry.min(h / 2).max(0);
        if rx == 0 && ry == 0 {
            self.fill_rect(x, y, w, h, color);
            return;
        }
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width as i32);
        let y1 = (y + h).min(self.height as i32);
        for py in y0..y1 {
            // Fast path: rows fully inside (not in corner zone)
            let in_top_corner = py < y + ry;
            let in_bot_corner = py >= y + h - ry;
            if !in_top_corner && !in_bot_corner {
                // Entire row is inside — draw as solid span
                for px in x0..x1 {
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], color[3]);
                }
                continue;
            }
            for px in x0..x1 {
                // Only corner columns need coverage calculation
                let in_left = px < x + rx;
                let in_right = px >= x + w - rx;
                if !in_left && !in_right {
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], color[3]);
                    continue;
                }
                let cov = Self::rounded_rect_coverage(px, py, x, y, w, h, rx, ry);
                if cov >= 0.99 {
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], color[3]);
                } else if cov > 0.01 {
                    let a = (color[3] as f32 * cov) as u8;
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], a);
                }
            }
        }
    }

    /// Draw a stroked rounded rectangle with anti-aliased edges
    pub fn stroke_rounded_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        rx: i32,
        ry: i32,
        line_width: u32,
        color: [u8; 4],
    ) {
        let rx = rx.min(w / 2).max(0);
        let ry = ry.min(h / 2).max(0);
        if rx == 0 && ry == 0 {
            self.stroke_rect(x, y, w, h, line_width, color);
            return;
        }
        let lw = line_width as i32;
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width as i32);
        let y1 = (y + h).min(self.height as i32);
        let irx = (rx - lw).max(0);
        let iry = (ry - lw).max(0);
        for py in y0..y1 {
            for px in x0..x1 {
                let outer = Self::rounded_rect_coverage(px, py, x, y, w, h, rx, ry);
                if outer < 0.01 {
                    continue;
                }
                let inner = Self::rounded_rect_coverage(
                    px,
                    py,
                    x + lw,
                    y + lw,
                    w - lw * 2,
                    h - lw * 2,
                    irx,
                    iry,
                );
                let cov = outer - inner; // stroke = outer minus inner
                if cov >= 0.99 {
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], color[3]);
                } else if cov > 0.01 {
                    let a = (color[3] as f32 * cov) as u8;
                    self.set_pixel(px as u32, py as u32, color[0], color[1], color[2], a);
                }
            }
        }
    }

    /// Draw text using embedded 8x8 bitmap font
    pub fn draw_text(&mut self, text: &str, x: i32, y: i32, color: [u8; 4]) {
        self.draw_text_scaled(text, x, y, 1.0, color);
    }

    /// Draw text using embedded 8x8 bitmap font with scaling
    pub fn draw_text_scaled(&mut self, text: &str, x: i32, y: i32, scale: f32, color: [u8; 4]) {
        let char_w = (8.0 * scale) as i32;
        let mut cx = x;
        for ch in text.chars() {
            let glyph_idx = (ch as usize).min(127);
            let glyph = &FONT_8X8[glyph_idx];
            for row in 0..8 {
                let byte = glyph[row as usize];
                for col in 0..8 {
                    if byte & (0x80 >> col) != 0 {
                        // Draw scaled pixel block
                        let px_start = cx + (col as f32 * scale) as i32;
                        let py_start = y + (row as f32 * scale) as i32;
                        let px_end = cx + ((col + 1) as f32 * scale) as i32;
                        let py_end = y + ((row + 1) as f32 * scale) as i32;
                        for py in py_start..py_end {
                            for px in px_start..px_end {
                                if px >= 0 && py >= 0 {
                                    self.set_pixel(
                                        px as u32, py as u32, color[0], color[1], color[2],
                                        color[3],
                                    );
                                }
                            }
                        }
                    }
                }
            }
            cx += char_w;
        }
    }
}

mod font;
pub use font::FONT_8X8;

#[cfg(test)]
mod tests;
