use crate::pixel_buffer::{PixelBuffer, StencilCompare};
use crate::shader_batch::ShaderBatch;

#[cfg(test)]
thread_local! { static REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

pub(crate) fn plain_enabled() -> bool {
    #[cfg(test)]
    if REFERENCE.with(std::cell::Cell::get) {
        return false;
    }
    static ENABLED: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("BALATRO_PLAIN_AFFINE").as_deref() != Ok("0"));
    *ENABLED
}

impl PixelBuffer {
    // The compiler emits separate loops for plain and shader-batched sprites.
    pub(crate) fn draw_nearest_affine<const SHADED: bool>(
        &mut self,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        white_mask: bool,
        replace: bool,
        batch: Option<&mut ShaderBatch>,
    ) {
        #[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
        if SHADED
            && batch.as_ref().is_some_and(|batch| {
                batch.draw_affine(
                    self,
                    source,
                    source_width,
                    region,
                    bounds,
                    inverse,
                    tint,
                    white_mask,
                    replace,
                )
            })
        {
            return;
        }
        #[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
        if !SHADED
            && crate::neon_sprite::draw(
                self,
                source,
                source_width,
                region,
                bounds,
                inverse,
                tint,
                white_mask,
                replace,
            )
        {
            return;
        }
        if crate::affine_span::enabled()
            && inverse.iter().chain(region.iter()).all(|v| v.is_finite())
            && inverse.iter().all(|v| v.abs() < 1_000_000.0)
            && bounds.iter().all(|v| (0..=16_384).contains(v))
            && region[2] > 0.0
            && region[3] > 0.0
        {
            self.nearest_affine_loop::<SHADED, true>(
                source,
                source_width,
                region,
                bounds,
                inverse,
                tint,
                white_mask,
                replace,
                batch,
            );
        } else {
            self.nearest_affine_loop::<SHADED, false>(
                source,
                source_width,
                region,
                bounds,
                inverse,
                tint,
                white_mask,
                replace,
                batch,
            );
        }
    }

    fn nearest_affine_loop<const SHADED: bool, const SPANS: bool>(
        &mut self,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        white_mask: bool,
        replace: bool,
        mut batch: Option<&mut ShaderBatch>,
    ) {
        if source_width == 0 {
            return;
        }
        let source_height = source.len() / 4 / source_width as usize;
        let [sx, sy, width, height] = region;
        let [x0, y0, x1, y1] = bounds;
        let [a, b, tx, c, d, ty] = inverse;
        let white_tint = tint == [255; 4];
        let u_axis = crate::affine_span::Axis::new(a, width);
        let v_axis = crate::affine_span::Axis::new(c, height);
        for y in y0..y1 {
            let yf = y as f32 + 0.5;
            let base_u = b * yf + tx;
            let base_v = d * yf + ty;
            let destination_row = y as usize * self.width as usize * 4;
            let (x0, x1) = if SPANS {
                let (start, end) = u_axis.clip(x0, x1, base_u);
                v_axis.clip(start, end, base_v)
            } else {
                (x0, x1)
            };
            for x in x0..x1 {
                let xf = x as f32 + 0.5;
                let u = a * xf + base_u;
                let v = c * xf + base_v;
                if !SPANS && (u < 0.0 || v < 0.0 || u >= width || v >= height) {
                    continue;
                }
                let column = (sx + u) as u32;
                let row = (sy + v) as u32;
                if column >= source_width || row as usize >= source_height {
                    continue;
                }
                let offset = (row as usize * source_width as usize + column as usize) * 4;
                let sample = &source[offset..offset + 4];
                let alpha = sample[3];
                if alpha == 0 {
                    continue;
                }
                if self.stencil_compare != StencilCompare::Disabled
                    && !self.stencil_test(x as u32, y as u32)
                {
                    continue;
                }
                let color = if white_tint {
                    [sample[0], sample[1], sample[2], alpha]
                } else if white_mask {
                    [
                        tint[0],
                        tint[1],
                        tint[2],
                        (u16::from(alpha) * u16::from(tint[3]) / 255) as u8,
                    ]
                } else {
                    [
                        (u16::from(sample[0]) * u16::from(tint[0]) / 255) as u8,
                        (u16::from(sample[1]) * u16::from(tint[1]) / 255) as u8,
                        (u16::from(sample[2]) * u16::from(tint[2]) / 255) as u8,
                        (u16::from(alpha) * u16::from(tint[3]) / 255) as u8,
                    ]
                };
                let destination = destination_row + x as usize * 4;
                if SHADED {
                    let batch = batch.as_deref_mut().expect("shader batch required");
                    batch.push(
                        color,
                        [u / width, v / height],
                        destination,
                        self.shader_colour_cache.as_deref_mut(),
                    );
                    if batch.full() {
                        self.flush_shader_batch(batch, replace);
                    }
                    continue;
                }
                if replace || ((self.blend == 0 || self.blend == 4) && color[3] == 255) {
                    self.pixels[destination..destination + 4].copy_from_slice(&color);
                } else if color[3] > 0 {
                    self.blend_at(destination, color[0], color[1], color[2], color[3]);
                }
            }
        }
        if SHADED {
            self.flush_shader_batch(batch.expect("shader batch required"), replace);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel_buffer::DissolveParams;

    fn reference(draw: impl FnOnce()) {
        struct Reset(bool);
        impl Drop for Reset {
            fn drop(&mut self) {
                REFERENCE.with(|flag| flag.set(self.0));
            }
        }
        let _reset = Reset(REFERENCE.with(|flag| flag.replace(true)));
        draw();
    }

    #[test]
    fn plain_loop_preserves_sampling_tint_blending_and_stencils() {
        let mut source = vec![0; 31 * 23 * 4];
        for (n, pixel) in source.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[n as u8, (n * 17) as u8, (n * 43) as u8, n as u8]);
        }
        for blend in 0..6 {
            for replace in [false, true] {
                for tint in [[255; 4], [131, 53, 244, 77], [255, 255, 255, 0]] {
                    for white_mask in [false, true] {
                        for inverse in [
                            [0.43, 0.12, -3.27, -0.1, 0.69, 2.5],
                            [-0.7, 0.2, 30.0, 0.14, 0.43, -1.0],
                            [2.3, 0.13, -11.0, -0.6, 1.75, 5.0],
                        ] {
                            for stencil in [false, true] {
                                let mut actual = PixelBuffer::new(47, 41);
                                actual.clear(0.1, 0.4, 0.7, 0.9);
                                actual.blend = blend;
                                actual.scissor = Some((2, 3, 41, 33));
                                if stencil {
                                    actual.stencil_compare = StencilCompare::Equal;
                                    actual.stencil_ref = 1;
                                    actual.stencil.fill(1);
                                    for n in (0..47 * 41).step_by(5) {
                                        actual.stencil[n] = 0;
                                    }
                                }
                                let mut expected = PixelBuffer::new(47, 41);
                                expected.copy_raster_source(&actual);
                                let draw = |target: &mut PixelBuffer| {
                                    target.draw_image_region_transformed(
                                        &source,
                                        31,
                                        3.25,
                                        2.75,
                                        24.0,
                                        18.0,
                                        (-2, -1, 49, 45),
                                        inverse,
                                        tint,
                                        replace,
                                        white_mask,
                                        DissolveParams::NONE,
                                    )
                                };
                                reference(|| draw(&mut expected));
                                draw(&mut actual);
                                assert_eq!(actual.pixels, expected.pixels);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn random_plain_sprites_match_reference_images() {
        let mut seed = 0x75632981_u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let source: Vec<u8> = (0..31 * 23 * 4).map(|_| next() as u8).collect();
        for _ in 0..512 {
            let mut actual = PixelBuffer::new(47, 41);
            actual.clear(0.2, 0.5, 0.8, 0.9);
            actual.blend = if next() & 1 == 0 { 0 } else { 4 };
            let mut expected = PixelBuffer::new(47, 41);
            expected.copy_raster_source(&actual);
            let inverse = [
                (next() as i32 % 4000) as f32 / 1024.0,
                (next() as i32 % 1000) as f32 / 1024.0,
                (next() as i32 % 30000) as f32 / 1024.0,
                (next() as i32 % 1000) as f32 / 1024.0,
                (next() as i32 % 4000) as f32 / 1024.0,
                (next() as i32 % 30000) as f32 / 1024.0,
            ];
            let sx = (next() as i32 % 10000) as f32 / 1024.0;
            let sy = (next() as i32 % 10000) as f32 / 1024.0;
            let width = (next() % 40000 + 1) as f32 / 1024.0;
            let height = (next() % 40000 + 1) as f32 / 1024.0;
            let tint = next().to_le_bytes();
            let replace = next() & 1 != 0;
            let white_mask = next() & 1 != 0;
            let draw = |target: &mut PixelBuffer| {
                target.draw_image_region_transformed(
                    &source,
                    31,
                    sx,
                    sy,
                    width,
                    height,
                    (-2, -1, 49, 45),
                    inverse,
                    tint,
                    replace,
                    white_mask,
                    DissolveParams::NONE,
                )
            };
            reference(|| draw(&mut expected));
            draw(&mut actual);
            assert_eq!(actual.pixels, expected.pixels, "inverse={inverse:?}");
        }
    }
}
