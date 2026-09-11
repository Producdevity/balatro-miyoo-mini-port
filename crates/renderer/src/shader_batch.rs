use crate::card_effects::ShaderPre;
use crate::shader_colour_cache::{card_hsl, ColourCache};

const CAPACITY: usize = 32;
#[cfg(all(
    test,
    target_arch = "arm",
    target_endian = "little",
    feature = "arm-neon"
))]
#[path = "shader_pair_tests.rs"]
mod pair_tests;
#[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
#[path = "shader_worker.rs"]
mod worker;
const BYTE_FRACTIONS: [f32; 256] = {
    let mut values = [0.0; 256];
    let mut n = 0;
    while n < values.len() {
        values[n] = n as f32 / 255.0;
        n += 1;
    }
    values
};

#[cfg(test)]
thread_local! { static SCALAR_REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(test)]
thread_local! { static FUSED_REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

// Layout shared with neon_card.c. Only this POD data crosses the C boundary.
#[repr(C)]
struct Uniforms {
    texture: [f32; 2],
    phase: f32,
    clock: f32,
    foil: [f32; 6],
    offsets: [f32; 6],
    sine: *const f32,
    byte_fractions: *const f32,
    noise_axes: u32,
}

#[repr(C)]
struct Samples {
    rgba: [u32; CAPACITY],
    u: [f32; CAPACITY],
    v: [f32; CAPACITY],
    h: [f32; CAPACITY],
    s: [f32; CAPACITY],
    l: [f32; CAPACITY],
}

pub(crate) struct ShaderBatch {
    uniforms: Uniforms,
    samples: Samples,
    pub destinations: [usize; CAPACITY],
    pub len: usize,
    effect: u8,
}

#[cfg(all(target_arch = "arm", feature = "arm-neon"))]
extern "C" {
    fn balatro_card_shader(
        effect: u32,
        uniforms: *const Uniforms,
        samples: *mut Samples,
        count: usize,
    );
}

#[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
unsafe extern "C" fn fill_hsl(
    context: *mut std::ffi::c_void,
    effect: u32,
    samples: *mut Samples,
    count: usize,
) {
    let cache = &mut *context.cast::<Option<&mut ColourCache>>();
    let samples = &mut *samples;
    for n in 0..count {
        let [r, g, b, _] = samples.rgba[n].to_le_bytes();
        let (h, s, l) = card_hsl(cache.as_deref_mut(), effect as u8, [r, g, b]);
        samples.h[n] = h;
        samples.s[n] = s;
        samples.l[n] = l;
    }
}

#[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
unsafe fn raster(
    effect: u8,
    sprite: &crate::neon_sprite::Sprite,
    uniforms: &Uniforms,
    source: *const u8,
    target: *mut u8,
    mut cache: Option<&mut ColourCache>,
) {
    extern "C" {
        fn balatro_shaded_sprite(
            effect: u32,
            sprite: *const crate::neon_sprite::Sprite,
            uniforms: *const Uniforms,
            source: *const u8,
            target: *mut u8,
            cache: *mut std::ffi::c_void,
            fill: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut Samples, usize),
        );
    }
    balatro_shaded_sprite(
        u32::from(effect),
        sprite,
        uniforms,
        source,
        target,
        (&mut cache as *mut Option<&mut ColourCache>).cast(),
        fill_hsl,
    );
}

impl ShaderBatch {
    #[cfg(all(
        feature = "layer-pairs",
        target_arch = "arm",
        target_endian = "little",
        feature = "arm-neon"
    ))]
    pub(crate) fn draw_pair(
        &self,
        target: &mut crate::pixel_buffer::PixelBuffer,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        overlay_source: &[u8],
        overlay_width: u32,
        overlay_region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        replace: bool,
        hidden: Option<[i32; 4]>,
    ) -> bool {
        use crate::neon_sprite::Sprite;
        let Some(base) = Sprite::new(
            target,
            source,
            source_width,
            region,
            bounds,
            inverse,
            tint,
            false,
            replace,
        ) else {
            return false;
        };
        let Some(overlay) = Sprite::new(
            target,
            overlay_source,
            overlay_width,
            overlay_region,
            bounds,
            inverse,
            tint,
            false,
            replace,
        ) else {
            return false;
        };
        for (base, overlay) in base.without(hidden).zip(overlay.without(hidden)) {
            if worker::draw_pair(
                self.effect,
                &base,
                &overlay,
                &self.uniforms,
                source,
                overlay_source,
                &mut target.pixels,
                target.shader_colour_cache.as_deref_mut(),
            ) {
                continue;
            }
            unsafe {
                raster_pair(
                    self.effect,
                    &base,
                    &overlay,
                    &self.uniforms,
                    source.as_ptr(),
                    overlay_source.as_ptr(),
                    target.pixels.as_mut_ptr(),
                    target.shader_colour_cache.as_deref_mut(),
                );
            }
        }
        true
    }

    #[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
    pub(crate) fn draw_affine(
        &self,
        target: &mut crate::pixel_buffer::PixelBuffer,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        white_mask: bool,
        replace: bool,
    ) -> bool {
        #[cfg(test)]
        if FUSED_REFERENCE.get() {
            return false;
        }
        static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            cfg!(test) || std::env::var("BALATRO_FUSED_SHADER").as_deref() == Ok("1")
        });
        if !*ENABLED {
            return false;
        }
        let Some(sprite) = crate::neon_sprite::Sprite::new(
            target,
            source,
            source_width,
            region,
            bounds,
            inverse,
            tint,
            white_mask,
            replace,
        ) else {
            return false;
        };
        if worker::draw(
            self.effect,
            &sprite,
            &self.uniforms,
            source,
            &mut target.pixels,
            target.shader_colour_cache.as_deref_mut(),
        ) {
            return true;
        }
        // Sprite validates both allocations and the destination bounds. The C
        // loop bounds-checks each source gather and uses at most 32 samples.
        unsafe {
            raster(
                self.effect,
                &sprite,
                &self.uniforms,
                source.as_ptr(),
                target.pixels.as_mut_ptr(),
                target.shader_colour_cache.as_deref_mut(),
            );
        }
        true
    }

    pub fn new(effect: u8, pre: &ShaderPre) -> Option<Self> {
        #[cfg(test)]
        if SCALAR_REFERENCE.with(std::cell::Cell::get) {
            return None;
        }
        static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var("BALATRO_SHADER_BATCH").as_deref() != Ok("0")
        });
        if !cfg!(all(target_arch = "arm", feature = "arm-neon"))
            || !*ENABLED
            || !(1..=6).contains(&effect)
        {
            return None;
        }
        let result = Self::create(effect, pre);
        let u = &result.uniforms;
        if u.texture
            .iter()
            .any(|v| !v.is_finite() || *v < 1.0 || *v > 65_536.0)
            || [u.phase, u.clock]
                .iter()
                .chain(u.foil.iter())
                .chain(u.offsets.iter())
                .any(|v| !v.is_finite() || v.abs() > 100_000.0)
        {
            return None;
        }
        Some(result)
    }

    fn create(effect: u8, pre: &ShaderPre) -> Self {
        static NUMERIC: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var("BALATRO_SHADER_NUMERIC").as_deref() != Ok("0")
        });
        Self {
            uniforms: Uniforms {
                texture: pre.texture_size,
                phase: match effect {
                    3 => pre.foil_r,
                    4 => pre.holo_x,
                    _ => pre.poly_x,
                },
                clock: if effect == 6 {
                    if pre.negative_invert {
                        1.0
                    } else {
                        0.0
                    }
                } else if effect == 3 {
                    pre.foil_g
                } else {
                    pre.poly_y
                },
                foil: [
                    pre.foil_rot_x,
                    pre.foil_rot_y,
                    pre.foil_rot_len,
                    pre.foil_inner_sin,
                    pre.foil_cos7,
                    pre.foil_cos3414,
                ],
                offsets: if effect == 4 {
                    pre.holo_off
                } else {
                    pre.poly_off
                },
                sine: pre.sine_table(),
                byte_fractions: if *NUMERIC {
                    BYTE_FRACTIONS.as_ptr()
                } else {
                    std::ptr::null()
                },
                noise_axes: {
                    static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
                        std::env::var("BALATRO_NOISE_AXES").as_deref() != Ok("0")
                    });
                    u32::from(*ENABLED)
                },
            },
            samples: Samples {
                rgba: [0; CAPACITY],
                u: [0.0; CAPACITY],
                v: [0.0; CAPACITY],
                h: [0.0; CAPACITY],
                s: [0.0; CAPACITY],
                l: [0.0; CAPACITY],
            },
            destinations: [0; CAPACITY],
            len: 0,
            effect,
        }
    }

    pub fn push(
        &mut self,
        color: [u8; 4],
        uv: [f32; 2],
        destination: usize,
        cache: Option<&mut ColourCache>,
    ) {
        let n = self.len;
        self.samples.rgba[n] = u32::from_le_bytes(color);
        self.samples.u[n] = uv[0];
        self.samples.v[n] = uv[1];
        self.destinations[n] = destination;
        if self.effect != 3 {
            let (h, s, l) = card_hsl(cache, self.effect, [color[0], color[1], color[2]]);
            self.samples.h[n] = h;
            self.samples.s[n] = s;
            self.samples.l[n] = l;
        }
        self.len += 1;
    }

    pub fn full(&self) -> bool {
        self.len == CAPACITY
    }

    pub fn shade(&mut self) {
        assert!(self.len <= CAPACITY);
        if self.len == 0 {
            return;
        }
        #[cfg(all(target_arch = "arm", feature = "arm-neon"))]
        unsafe {
            // Arrays hold 32 initialized entries; at most three padding lanes are ignored.
            balatro_card_shader(
                u32::from(self.effect),
                &self.uniforms,
                &mut self.samples,
                self.len.next_multiple_of(4),
            );
        }
    }

    pub fn color(&self, n: usize) -> [u8; 4] {
        self.samples.rgba[n].to_le_bytes()
    }
}

#[cfg(all(
    feature = "layer-pairs",
    target_arch = "arm",
    target_endian = "little",
    feature = "arm-neon"
))]
unsafe fn raster_pair(
    effect: u8,
    base: &crate::neon_sprite::Sprite,
    overlay: &crate::neon_sprite::Sprite,
    uniforms: &Uniforms,
    source: *const u8,
    overlay_source: *const u8,
    target: *mut u8,
    mut cache: Option<&mut ColourCache>,
) {
    extern "C" {
        fn balatro_shaded_sprite_pair(
            effect: u32,
            base: *const crate::neon_sprite::Sprite,
            overlay: *const crate::neon_sprite::Sprite,
            uniforms: *const Uniforms,
            source: *const u8,
            overlay_source: *const u8,
            target: *mut u8,
            cache: *mut std::ffi::c_void,
            fill: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut Samples, usize),
        );
    }
    balatro_shaded_sprite_pair(
        u32::from(effect),
        base,
        overlay,
        uniforms,
        source,
        overlay_source,
        target,
        (&mut cache as *mut Option<&mut ColourCache>).cast(),
        fill_hsl,
    );
}

#[cfg(test)]
#[test]
fn byte_fractions_preserve_division_rounding() {
    for (byte, &fraction) in BYTE_FRACTIONS.iter().enumerate() {
        let runtime_byte = std::hint::black_box(byte as u8);
        assert_eq!(
            fraction.to_bits(),
            (f32::from(runtime_byte) / 255.0).to_bits()
        );
        assert_eq!(
            u32::from(runtime_byte) * 171 >> 9,
            u32::from(runtime_byte) / 3
        );
    }
}

#[cfg(all(test, target_arch = "arm", feature = "arm-neon"))]
#[path = "neon_shader_tests.rs"]
mod fused_tests;

#[cfg(all(test, target_arch = "arm", feature = "arm-neon"))]
mod tests {
    use super::*;
    use crate::card_effects::CardShaderInputs;

    fn scalar_reference<T>(draw: impl FnOnce() -> T) -> T {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                SCALAR_REFERENCE.set(self.0);
            }
        }
        let _restore = Restore(SCALAR_REFERENCE.replace(true));
        draw()
    }

    #[test]
    fn numeric_paths_match_for_every_alpha_and_byte_value() {
        for effect in 1..=6 {
            let pre = ShaderPre::compute(
                effect,
                CardShaderInputs {
                    phase: 1.75,
                    clock: 49.0,
                    seed: 123.5,
                },
            );
            for first in (0..256).step_by(CAPACITY) {
                let mut direct = ShaderBatch::create(effect, &pre);
                direct.uniforms.byte_fractions = std::ptr::null();
                let mut numeric = ShaderBatch::create(effect, &pre);
                numeric.uniforms.byte_fractions = BYTE_FRACTIONS.as_ptr();
                for n in 0..CAPACITY {
                    let value = (first + n) as u8;
                    let color = [value, value.wrapping_mul(17), 255 - value, value];
                    let uv = [n as f32 / 32.0, value as f32 / 256.0];
                    direct.push(color, uv, n, None);
                    numeric.push(color, uv, n, None);
                }
                direct.shade();
                numeric.shade();
                assert_eq!(
                    numeric.samples.rgba, direct.samples.rgba,
                    "effect={effect} first={first}"
                );
            }
        }
    }

    #[test]
    fn batches_preserve_filtered_tinted_clipped_and_blended_draws() {
        use crate::pixel_buffer::{DissolveParams, PixelBuffer, StencilCompare};
        let source: Vec<u8> = (0..25 * 33 * 4).map(|n| (n * 71 + n / 19) as u8).collect();
        for effect in 1..=6 {
            for filter in [false, true] {
                for blend in 0..=5 {
                    for angle in [-0.35_f32, 0.0, 0.18] {
                        for dissolve in [0.0, 0.4] {
                            let draw = || {
                                let mut target = PixelBuffer::new(79, 61);
                                target.clear(0.2, 0.4, 0.3, 0.8);
                                target.filter_linear = filter;
                                target.blend = blend;
                                target.scissor = Some((3, 7, 63, 51));
                                target.stencil_compare = StencilCompare::NotEqual;
                                target.stencil_ref = 1;
                                for n in (0..target.stencil.len()).step_by(7) {
                                    target.stencil[n] = 1;
                                }
                                let params = DissolveParams {
                                    shader_effect: effect,
                                    dissolve,
                                    shader_inputs: CardShaderInputs {
                                        phase: 2.3,
                                        clock: 42.0,
                                        seed: 327.125,
                                    },
                                    ..DissolveParams::NONE
                                };
                                target.draw_image_region(
                                    &source,
                                    25,
                                    33,
                                    1.0,
                                    2.0,
                                    21.0,
                                    29.0,
                                    4.3,
                                    5.1,
                                    1.4,
                                    1.2,
                                    [177, 219, 31, 129],
                                    false,
                                    false,
                                    params,
                                );
                                let c = angle.cos() * 0.6;
                                let s = angle.sin() * 0.6;
                                target.draw_image_region_transformed(
                                    &source,
                                    25,
                                    1.0,
                                    2.0,
                                    21.0,
                                    29.0,
                                    (0, 0, 79, 61),
                                    [c, s, -5.2, -s, c, -2.1],
                                    [255; 4],
                                    blend == 1,
                                    false,
                                    params,
                                );
                                target.pixels
                            };
                            let expected = scalar_reference(draw);
                            assert_eq!(draw(), expected, "effect={effect} filter={filter} blend={blend} angle={angle} dissolve={dissolve}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn vector_shader_matches_scalar_pixels_across_colours_coordinates_and_time() {
        let mut random = 0x437de923_u32;
        for effect in 1..=6 {
            for clock in [0.0, 1.25, 31.5, 1234.0, 9123.5] {
                for texture in [[71.0, 95.0], [142.0, 190.0], [93.0, 93.0]] {
                    let mut pre = ShaderPre::compute(
                        effect,
                        CardShaderInputs {
                            phase: clock / 28.0,
                            clock,
                            seed: 987.125,
                        },
                    );
                    pre.texture_size = texture;
                    for iteration in 0..64 {
                        let mut batch = ShaderBatch::create(effect, &pre);
                        let mut expected = [[0; 4]; CAPACITY];
                        for (n, result) in expected.iter_mut().enumerate() {
                            random ^= random << 13;
                            random ^= random >> 17;
                            random ^= random << 5;
                            let [mut r, mut g, mut b, mut a] = random.to_le_bytes();
                            let u = ((iteration * 32 + n) % 71) as f32 / 71.0;
                            let v = ((iteration * 17 + n * 3) % 95) as f32 / 95.0;
                            batch.push([r, g, b, a], [u, v], n, None);
                            crate::pixel_buffer::apply_card_shader(
                                effect, &pre, None, None, u, v, 0.0, 0.0, &mut r, &mut g, &mut b,
                                &mut a,
                            );
                            *result = [r, g, b, a];
                        }
                        batch.shade();
                        for (n, expected) in expected.iter().enumerate() {
                            assert_eq!(&batch.color(n), expected, "effect={effect} clock={clock} texture={texture:?} iteration={iteration} lane={n}");
                        }
                    }
                }
            }
        }
    }
}
