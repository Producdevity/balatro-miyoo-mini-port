use crate::pixel_buffer::{PixelBuffer, StencilCompare};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Sprite {
    source_width: u32,
    source_height: u32,
    target_width: u32,
    bounds: [i32; 4],
    region: [f32; 4],
    inverse: [f32; 6],
    tint: u32,
    white_mask: u32,
    premultiplied: u32,
    replace: u32,
    alpha_shortcuts: u32,
    row_spans: u32,
    packed_pixels: u32,
}

extern "C" {
    fn balatro_nearest_sprite(sprite: *const Sprite, source: *const u8, target: *mut u8);
    fn balatro_prepared_sprite(
        sprite: *const Sprite,
        source: *const u8,
        target: *mut u8,
        origin_x: u32,
        origin_y: u32,
    );
}

pub(crate) fn draw_prepared(
    target: &mut PixelBuffer,
    source: &[u8],
    stride: u32,
    origin: [u32; 2],
    region: [u32; 2],
    bounds: [i32; 4],
    inverse: [f32; 6],
) -> bool {
    static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        cfg!(test) || std::env::var("BALATRO_PREPARED_NEON").as_deref() != Ok("0")
    });
    if !*ENABLED {
        return false;
    }
    let Some(sprite) = Sprite::new(
        target,
        source,
        stride,
        [
            origin[0] as f32,
            origin[1] as f32,
            region[0] as f32,
            region[1] as f32,
        ],
        bounds,
        inverse,
        [255; 4],
        false,
        false,
    ) else {
        return false;
    };
    // The loop checks cropped coordinates after the original atlas addition.
    // Sprite validates target bounds; source gathers stay inside the crop.
    unsafe {
        balatro_prepared_sprite(
            &sprite,
            source.as_ptr(),
            target.pixels.as_mut_ptr(),
            origin[0],
            origin[1],
        );
    }
    true
}

pub(crate) fn draw(
    target: &mut PixelBuffer,
    source: &[u8],
    source_width: u32,
    region: [f32; 4],
    bounds: [i32; 4],
    inverse: [f32; 6],
    tint: [u8; 4],
    white_mask: bool,
    replace: bool,
) -> bool {
    static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        cfg!(test) || std::env::var("BALATRO_NEON_SPRITES").as_deref() != Ok("0")
    });
    if !*ENABLED {
        return false;
    }
    let Some(sprite) = Sprite::new(
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
    // The destination rectangle is inside its allocation. The native loop
    // checks each gathered source coordinate and never reads a partial pixel.
    unsafe {
        balatro_nearest_sprite(&sprite, source.as_ptr(), target.pixels.as_mut_ptr());
    }
    true
}

impl Sprite {
    #[cfg(feature = "layer-pairs")]
    pub(crate) fn without(&self, hidden: Option<[i32; 4]>) -> impl Iterator<Item = Self> {
        let [l, t, r, b] = self.bounds;
        let hidden = hidden
            .map(|[x0, y0, x1, y1]| [x0.max(l), y0.max(t), x1.min(r), y1.min(b)])
            .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1);
        let parts = match hidden {
            Some([x0, y0, x1, y1]) => [
                [l, t, r, y0],
                [l, y1, r, b],
                [l, y0, x0, y1],
                [x1, y0, r, y1],
            ],
            None => [self.bounds, [0; 4], [0; 4], [0; 4]],
        };
        let sprite = *self;
        parts
            .into_iter()
            .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1)
            .map(move |bounds| Self { bounds, ..sprite })
    }

    pub(crate) fn split_rows(&self, minimum_pixels: i64) -> Option<(Self, Self)> {
        let [left, top, right, bottom] = self.bounds;
        let rows = bottom - top;
        if rows < 2 || right <= left || i64::from(right - left) * i64::from(rows) < minimum_pixels {
            return None;
        }
        let mut first = *self;
        let mut second = *self;
        let middle = top + rows / 2;
        first.bounds[3] = middle;
        second.bounds[1] = middle;
        Some((first, second))
    }

    pub(crate) fn new(
        target: &PixelBuffer,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        white_mask: bool,
        replace: bool,
    ) -> Option<Self> {
        static ALPHA_SHORTCUTS: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var("BALATRO_SPRITE_ALPHA").as_deref() != Ok("0")
        });
        static ROW_SPANS: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var("BALATRO_SPRITE_SPANS").as_deref() == Ok("1")
        });
        static PACKED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            cfg!(test) || std::env::var("BALATRO_PACKED_PIXELS").as_deref() == Ok("1")
        });
        if source_width == 0
            || target.stencil_write_mode
            || target.stencil_compare != StencilCompare::Disabled
            || (!replace && !matches!(target.blend, 0 | 4))
            || !inverse
                .iter()
                .chain(region.iter())
                .all(|v| v.is_finite() && v.abs() < 1_000_000.0)
            || inverse
                .iter()
                .chain(region.iter())
                .any(|v| *v != 0.0 && v.abs() < 1e-20)
            || region[2] <= 0.0
            || region[3] <= 0.0
        {
            return None;
        }
        let [x0, y0, x1, y1] = bounds;
        if x0 < 0
            || y0 < 0
            || x1 > target.width as i32
            || y1 > target.height as i32
            || bounds.iter().any(|v| !(0..=16_384).contains(v))
        {
            return None;
        }
        let required = (target.width as usize)
            .checked_mul(target.height as usize)
            .and_then(|n| n.checked_mul(4));
        if required.is_none_or(|n| n > target.pixels.len()) {
            return None;
        }
        let source_height = source.len() / 4 / source_width as usize;
        let Ok(source_height) = u32::try_from(source_height) else {
            return None;
        };
        Some(Self {
            source_width,
            source_height,
            target_width: target.width,
            bounds,
            region,
            inverse,
            tint: u32::from_le_bytes(tint),
            white_mask: u32::from(white_mask),
            premultiplied: u32::from(target.blend == 4),
            replace: u32::from(replace),
            alpha_shortcuts: u32::from(*ALPHA_SHORTCUTS),
            row_spans: u32::from(*ROW_SPANS),
            packed_pixels: u32::from(*PACKED),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_spans_match_checked_gathers_at_edges_and_random_transforms() {
        let source: Vec<u8> = (0..17 * 13 * 4 + 1)
            .map(|n| (n * 37 + n / 19) as u8)
            .collect();
        let mut random = 0x783aed21_u32;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random
        };
        for iteration in 0..8000 {
            let scale = match iteration % 8 {
                0 => 0.0,
                1 => -0.0,
                2 => 1.0,
                3 => -1.0,
                _ => (next() as i32 % 8192) as f32 / 2048.0,
            };
            let inverse = [
                scale,
                (next() as i32 % 8192) as f32 / 2048.0,
                (next() as i32 % 2048) as f32 / 128.0,
                (next() as i32 % 8192) as f32 / 2048.0,
                scale,
                (next() as i32 % 2048) as f32 / 128.0,
            ];
            let left = (next() % 43) as i32;
            let right = left + (next() % (44 - left as u32)) as i32;
            let mut sprite = Sprite {
                source_width: 17,
                source_height: 13,
                target_width: 43,
                bounds: [left, 0, right, 31],
                region: [
                    (next() as i32 % 256) as f32 / 64.0,
                    (next() as i32 % 256) as f32 / 64.0,
                    17.0,
                    13.0,
                ],
                inverse,
                tint: next(),
                white_mask: next() % 2,
                premultiplied: next() % 2,
                replace: next() % 2,
                alpha_shortcuts: next() % 2,
                row_spans: 0,
                packed_pixels: 0,
            };
            let mut expected = [33, 97, 173, 137].repeat(43 * 31 + 3);
            let mut actual = expected.clone();
            // Deliberately unaligned source storage and extra target sentinels.
            unsafe {
                balatro_nearest_sprite(&sprite, source.as_ptr().add(1), expected.as_mut_ptr());
                sprite.row_spans = 1;
                sprite.packed_pixels = next() % 2;
                balatro_nearest_sprite(&sprite, source.as_ptr().add(1), actual.as_mut_ptr());
            }
            assert_eq!(
                actual, expected,
                "iteration={iteration} inverse={inverse:?} region={:?}",
                sprite.region
            );
        }
    }

    #[test]
    fn alpha_shortcuts_match_full_blending() {
        for alpha in 0_u8..=255 {
            let source = [31, 113, 233, alpha].repeat(7 * 5);
            for tint in [[255; 4], [123, 53, 219, 255], [73, 0, 211, 77], [0; 4]] {
                for premultiplied in [0, 1] {
                    for replace in [0, 1] {
                        for white_mask in [0, 1] {
                            for tx in [0.0, -1.25, 30.0] {
                                let mut sprite = Sprite {
                                    source_width: 7,
                                    source_height: 5,
                                    target_width: 11,
                                    bounds: [0, 0, 11, 9],
                                    region: [0.0, 0.0, 7.0, 5.0],
                                    inverse: [0.71, 0.13, tx, -0.17, 0.61, 0.5],
                                    tint: u32::from_le_bytes(tint),
                                    white_mask,
                                    premultiplied,
                                    replace,
                                    alpha_shortcuts: 0,
                                    row_spans: 0,
                                    packed_pixels: 0,
                                };
                                let mut expected = [33, 97, 173, 137].repeat(11 * 9);
                                let mut actual = expected.clone();
                                unsafe {
                                    balatro_nearest_sprite(
                                        &sprite,
                                        source.as_ptr(),
                                        expected.as_mut_ptr(),
                                    );
                                    sprite.alpha_shortcuts = 1;
                                    sprite.packed_pixels = 1;
                                    balatro_nearest_sprite(
                                        &sprite,
                                        source.as_ptr(),
                                        actual.as_mut_ptr(),
                                    );
                                }
                                assert_eq!(actual, expected, "alpha={alpha} tint={tint:?} premultiplied={premultiplied} replace={replace} mask={white_mask} tx={tx}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn unsupported_draws_leave_the_target_unchanged() {
        let source = [255; 4 * 4 * 4];
        for case in 0..5 {
            let mut target = PixelBuffer::new(4, 4);
            target.clear(0.1, 0.4, 0.7, 0.5);
            let expected = target.pixels.clone();
            let mut inverse = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
            match case {
                0 => target.stencil_write_mode = true,
                1 => target.stencil_compare = StencilCompare::Equal,
                2 => target.blend = 2,
                3 => inverse[0] = f32::NAN,
                _ => inverse[0] = f32::MIN_POSITIVE,
            }
            assert!(!draw(
                &mut target,
                &source,
                4,
                [0.0, 0.0, 4.0, 4.0],
                [0, 0, 4, 4],
                inverse,
                [255; 4],
                false,
                false,
            ));
            assert_eq!(target.pixels, expected);
        }
    }

    #[test]
    fn partial_vectors_preserve_transparent_pixels_and_blend_rounding() {
        let mut source = vec![0; 17 * 11 * 4 + 3];
        for (n, pixel) in source.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[n as u8, (n * 31) as u8, (n * 71) as u8, (n * 17) as u8]);
        }
        for width in 1..=11 {
            for blend in [0, 4] {
                for replace in [false, true] {
                    for tint in [[255; 4], [121, 37, 87, 0], [53, 117, 255, 177]] {
                        for white_mask in [false, true] {
                            let mut target = PixelBuffer::new(width, 13);
                            target.clear(0.1, 0.4, 0.7, 0.5);
                            target.blend = blend;
                            let mut expected = target.pixels.clone();
                            let inverse = [0.91, 0.13, -1.5, -0.2, 0.73, -0.5];
                            for y in 0..13 {
                                for x in 0..width {
                                    let u = inverse[0] * (x as f32 + 0.5)
                                        + (inverse[1] * (y as f32 + 0.5) + inverse[2]);
                                    let v = inverse[3] * (x as f32 + 0.5)
                                        + (inverse[4] * (y as f32 + 0.5) + inverse[5]);
                                    if u < 0.0 || v < 0.0 || u >= 15.0 || v >= 9.0 {
                                        continue;
                                    }
                                    let si = (((1.25 + v) as usize) * 17 + (1.5 + u) as usize) * 4;
                                    let sample = &source[si..si + 4];
                                    if sample[3] == 0 {
                                        continue;
                                    }
                                    let mut color: [u8; 4] = sample.try_into().unwrap();
                                    if tint != [255; 4] {
                                        for ch in 0..4 {
                                            color[ch] = if white_mask && ch < 3 {
                                                tint[ch]
                                            } else {
                                                (u32::from(sample[ch]) * u32::from(tint[ch]) / 255)
                                                    as u8
                                            };
                                        }
                                    }
                                    let di = (y * width + x) as usize * 4;
                                    if replace {
                                        expected[di..di + 4].copy_from_slice(&color);
                                    } else if color[3] > 0 {
                                        let alpha = u32::from(color[3]);
                                        for ch in 0..3 {
                                            expected[di + ch] = if blend == 4 {
                                                (u32::from(color[ch])
                                                    + u32::from(expected[di + ch]) * (255 - alpha)
                                                        / 255)
                                                    .min(255)
                                                    as u8
                                            } else {
                                                ((u32::from(color[ch]) * alpha
                                                    + u32::from(expected[di + ch]) * (255 - alpha))
                                                    / 255)
                                                    as u8
                                            };
                                        }
                                        expected[di + 3] = (alpha
                                            + u32::from(expected[di + 3]) * (255 - alpha) / 255)
                                            as u8;
                                    }
                                }
                            }
                            assert!(draw(
                                &mut target,
                                &source,
                                17,
                                [1.5, 1.25, 15.0, 9.0],
                                [0, 0, width as i32, 13],
                                inverse,
                                tint,
                                white_mask,
                                replace
                            ));
                            assert_eq!(target.pixels, expected, "width={width} blend={blend} replace={replace} tint={tint:?} mask={white_mask}");
                        }
                    }
                }
            }
        }
    }
}
