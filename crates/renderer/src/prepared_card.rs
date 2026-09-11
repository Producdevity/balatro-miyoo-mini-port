use crate::card_effects::{CardShaderInputs, ShaderPre};
use crate::pixel_buffer::{apply_card_shader, PixelBuffer, StencilCompare};
use crate::shader_colour_cache::ColourCache;

/// A fixed card effect evaluated at source texels, before nearest-neighbor drawing.
pub struct PreparedCard {
    origin: [u32; 2],
    region: [u32; 2],
    stride: u32,
    pixels: Vec<u8>,
    coverage: Vec<u8>,
}

impl PreparedCard {
    pub fn new(
        source: &[u8],
        source_width: u32,
        region: [u32; 4],
        effect: u8,
        invert: bool,
    ) -> Option<Self> {
        if !matches!(effect, 1 | 6) || source_width == 0 {
            return None;
        }
        let source_height = u32::try_from(
            source
                .len()
                .checked_div((source_width as usize).checked_mul(4)?)?,
        )
        .ok()?;
        let [x, y, width, height] = region;
        if width == 0 || height == 0 || width > 256 || height > 256 {
            return None;
        }
        let right = x.checked_add(width)?;
        let bottom = y.checked_add(height)?;
        if right > source_width || bottom > source_height {
            return None;
        }
        // Preserve the atlas-edge sample when x + u rounds up to the next texel.
        let stride = width + u32::from(right < source_width);
        let rows = height + u32::from(bottom < source_height);
        let count = (stride * rows) as usize;
        let mut pixels = vec![0; count * 4];
        let mut coverage = vec![0; count.div_ceil(8)];
        let pre = ShaderPre::compute(
            effect,
            CardShaderInputs {
                clock: f32::from(invert),
                ..CardShaderInputs::default()
            },
        );
        let mut colours = ColourCache::new();
        for row in 0..rows {
            for col in 0..stride {
                let source_offset =
                    ((y + row) as usize * source_width as usize + (x + col) as usize) * 4;
                let [mut r, mut g, mut b, mut a] =
                    source[source_offset..source_offset + 4].try_into().ok()?;
                let n = (row * stride + col) as usize;
                if a != 0 {
                    coverage[n / 8] |= 1 << (n % 8);
                    apply_card_shader(
                        effect,
                        &pre,
                        Some(&mut colours),
                        None,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        &mut r,
                        &mut g,
                        &mut b,
                        &mut a,
                    );
                }
                pixels[n * 4..n * 4 + 4].copy_from_slice(&[r, g, b, a]);
            }
        }
        Some(Self {
            origin: [x, y],
            region: [width, height],
            stride,
            pixels,
            coverage,
        })
    }

    pub fn bytes(&self) -> usize {
        self.pixels.len() + self.coverage.len()
    }

    pub fn draw(
        &self,
        target: &mut PixelBuffer,
        bounds: [i32; 4],
        inverse: [f32; 6],
        replace: bool,
    ) {
        assert!(!target.filter_linear && !target.stencil_write_mode);
        let [mut x0, mut y0, mut x1, mut y1] = bounds;
        x0 = x0.max(0);
        y0 = y0.max(0);
        x1 = x1.min(target.width as i32);
        y1 = y1.min(target.height as i32);
        if let Some((x, y, w, h)) = target.scissor {
            x0 = x0.max(x);
            y0 = y0.max(y);
            x1 = x1.min(x.saturating_add_unsigned(w));
            y1 = y1.min(y.saturating_add_unsigned(h));
        }
        #[cfg(all(target_arch = "arm", feature = "arm-neon"))]
        if !replace
            && crate::neon_sprite::draw_prepared(
                target,
                &self.pixels,
                self.stride,
                self.origin,
                self.region,
                [x0, y0, x1, y1],
                inverse,
            )
        {
            return;
        }
        self.draw_scalar(target, [x0, y0, x1, y1], inverse, replace);
    }

    fn draw_scalar(
        &self,
        target: &mut PixelBuffer,
        [x0, y0, x1, y1]: [i32; 4],
        inverse: [f32; 6],
        replace: bool,
    ) {
        let [a, b, tx, c, d, ty] = inverse;
        for y in y0..y1 {
            let yf = y as f32 + 0.5;
            let base_u = b * yf + tx;
            let base_v = d * yf + ty;
            for x in x0..x1 {
                let xf = x as f32 + 0.5;
                let u = a * xf + base_u;
                let v = c * xf + base_v;
                if !(u >= 0.0 && v >= 0.0 && u < self.region[0] as f32 && v < self.region[1] as f32)
                {
                    continue;
                }
                let column = (self.origin[0] as f32 + u) as u32 - self.origin[0];
                let row = (self.origin[1] as f32 + v) as u32 - self.origin[1];
                let n = row as usize * self.stride as usize + column as usize;
                if column >= self.stride
                    || n >= self.pixels.len() / 4
                    || self.coverage[n / 8] & (1 << (n % 8)) == 0
                {
                    continue;
                }
                if target.stencil_compare != StencilCompare::Disabled
                    && !target.stencil_test(x as u32, y as u32)
                {
                    continue;
                }
                let di = (y as usize * target.width as usize + x as usize) * 4;
                let [r, g, b, alpha] = self.pixels[n * 4..n * 4 + 4].try_into().unwrap();
                if replace || ((target.blend == 0 || target.blend == 4) && alpha == 255) {
                    target.pixels[di..di + 4].copy_from_slice(&[r, g, b, alpha]);
                } else if alpha != 0 {
                    target.blend_at(di, r, g, b, alpha);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel_buffer::DissolveParams;

    #[cfg(all(target_arch = "arm", feature = "arm-neon"))]
    #[test]
    fn prepared_neon_matches_scalar_at_atlas_edges_and_low_alpha() {
        let mut random = 0xc03247b1_u32;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random
        };
        let mut source = vec![0; 512 * 128 * 4];
        for pixel in source.chunks_exact_mut(4) {
            let value = next();
            pixel.copy_from_slice(&value.to_le_bytes());
            pixel[3] = [0, 1, 2, 127, 254, 255][value as usize % 6];
        }
        for case in 0..3000 {
            let origin = [0, 7, 409, 487][case % 4];
            let region = [origin, (next() % 100), 25, 28];
            let effect = if case % 2 == 0 { 1 } else { 6 };
            let prepared = PreparedCard::new(&source, 512, region, effect, case % 3 == 0).unwrap();
            let scale = [0.0, 0.5, 0.8, 1.0, -0.7][case % 5];
            let inverse = [
                scale,
                (next() as i32 % 200) as f32 / 1024.0,
                (next() as i32 % 100) as f32 / 32.0,
                (next() as i32 % 200) as f32 / 1024.0,
                scale,
                (next() as i32 % 100) as f32 / 32.0,
            ];
            let mut expected = PixelBuffer::new(43, 31);
            expected.clear(0.3, 0.7, 0.2, 0.6);
            expected.pixels.extend_from_slice(&[19, 23, 31, 37]);
            expected.blend = if case / 2 % 2 == 0 { 0 } else { 4 };
            let mut actual = PixelBuffer::new(43, 31);
            actual.pixels.extend_from_slice(&[0; 4]);
            actual.copy_raster_source(&expected);
            let bounds = [case as i32 % 5, 2, 41, 29];
            prepared.draw_scalar(&mut expected, bounds, inverse, false);
            assert!(crate::neon_sprite::draw_prepared(
                &mut actual,
                &prepared.pixels,
                prepared.stride,
                prepared.origin,
                prepared.region,
                bounds,
                inverse,
            ));
            assert_eq!(
                actual.pixels, expected.pixels,
                "case={case} region={region:?}"
            );
        }
    }

    #[test]
    fn prepared_texels_preserve_affine_blending_clipping_and_atlas_coordinates() {
        let source: Vec<u8> = (0..43 * 39 * 4).map(|n| (n * 37 + n / 7) as u8).collect();
        for effect in [1, 6] {
            for invert in [false, true] {
                for region in [[0, 0, 43, 39], [7, 5, 25, 29], [18, 10, 25, 29]] {
                    let prepared = PreparedCard::new(&source, 43, region, effect, invert).unwrap();
                    for blend in 0..=5 {
                        for replace in [false, true] {
                            for angle in [-0.25_f32, 0.0, 0.08] {
                                let mut actual = PixelBuffer::new(73, 61);
                                actual.clear(0.3, 0.7, 0.2, 0.6);
                                actual.blend = blend;
                                actual.scissor = Some((2, 3, 69, 55));
                                actual.stencil_compare = StencilCompare::NotEqual;
                                actual.stencil_ref = 1;
                                for n in (0..actual.stencil.len()).step_by(11) {
                                    actual.stencil[n] = 1;
                                }
                                let mut expected = PixelBuffer::new(73, 61);
                                expected.copy_raster_source(&actual);
                                let inv = [
                                    angle.cos() * 0.8,
                                    angle.sin(),
                                    -3.7,
                                    -angle.sin(),
                                    angle.cos() * 0.7,
                                    1.9,
                                ];
                                expected.draw_image_region_transformed(
                                    &source,
                                    43,
                                    region[0] as f32,
                                    region[1] as f32,
                                    region[2] as f32,
                                    region[3] as f32,
                                    (-3, -7, 80, 65),
                                    inv,
                                    [255; 4],
                                    replace,
                                    false,
                                    DissolveParams {
                                        shader_effect: effect,
                                        shader_inputs: CardShaderInputs {
                                            clock: f32::from(invert),
                                            ..CardShaderInputs::default()
                                        },
                                        ..DissolveParams::NONE
                                    },
                                );
                                prepared.draw(&mut actual, [-3, -7, 80, 65], inv, replace);
                                assert_eq!(actual.pixels, expected.pixels, "effect={effect} invert={invert} region={region:?} blend={blend} replace={replace} angle={angle}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_animated_effects_and_invalid_regions() {
        let source = vec![255; 16 * 16 * 4];
        for effect in [0, 2, 3, 4, 5, 7, 255] {
            assert!(PreparedCard::new(&source, 16, [0, 0, 16, 16], effect, false).is_none());
        }
        for region in [
            [0, 0, 0, 16],
            [1, 0, 16, 16],
            [0, 1, 16, 16],
            [u32::MAX, 0, 16, 16],
        ] {
            assert!(PreparedCard::new(&source, 16, region, 1, false).is_none());
        }
    }
}
