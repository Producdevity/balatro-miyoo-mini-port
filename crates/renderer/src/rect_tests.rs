use crate::pixel_buffer::{PixelBuffer, StencilCompare};

#[test]
fn clear_writes_every_pixel_without_changing_stencil_or_clipping() {
    for (width, height) in [(0, 0), (1, 1), (17, 9), (640, 480)] {
        let mut buffer = PixelBuffer::new(width, height);
        buffer.scissor = Some((1, 2, 3, 4));
        buffer.blend = 3;
        buffer.stencil_compare = StencilCompare::Equal;
        buffer.stencil.fill(37);
        for (color, expected) in [
            ([0.0; 4], [0; 4]),
            ([1.0; 4], [255; 4]),
            ([0.216, 0.259, 0.267, 1.0], [55, 66, 68, 255]),
            ([-1.0, 0.5, 2.0, 0.0], [0, 127, 255, 0]),
            (
                [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0],
                [0, 255, 0, 255],
            ),
        ] {
            buffer.pixels.fill(93);
            buffer.clear(color[0], color[1], color[2], color[3]);
            assert!(buffer.pixels.chunks_exact(4).all(|pixel| pixel == expected));
            assert!(buffer.stencil.iter().all(|&value| value == 37));
            assert_eq!(buffer.scissor, Some((1, 2, 3, 4)));
            assert_eq!(buffer.blend, 3);
            assert!(buffer.stencil_compare == StencilCompare::Equal);
        }
    }
}

#[test]
fn rectangle_fill_matches_individual_pixels() {
    for (width, height) in [(1, 1), (17, 9), (65, 13)] {
        for scissor in [None, Some((2, 1, 11, 7)), Some((-3, -2, 5, 5))] {
            for blend in 0..6 {
                for stencil in 0..3 {
                    let mut original = PixelBuffer::new(width, height);
                    for (n, byte) in original.pixels.iter_mut().enumerate() {
                        *byte = (n * 37 + 11) as u8;
                    }
                    original.scissor = scissor;
                    original.blend = blend;
                    original.stencil_ref = 3;
                    original.stencil_write_mode = stencil == 1;
                    if stencil == 2 {
                        original.stencil_compare = StencilCompare::Equal;
                    }
                    for (n, byte) in original.stencil.iter_mut().enumerate() {
                        *byte = (n % 5) as u8;
                    }
                    for x in [-3, 0, 1, width as i32 - 2, width as i32] {
                        for w in [-1, 0, 1, 3, 17, width as i32 + 3] {
                            for color in [[0, 0, 0, 255], [31, 113, 233, 255], [89; 4], [0; 4]] {
                                for (y, h) in [
                                    (-1, height as i32 + 2),
                                    (height as i32 - 1, 1),
                                    (height as i32, 3),
                                    (0, 0),
                                    (0, -1),
                                ] {
                                    let mut actual = PixelBuffer::new(width, height);
                                    actual.copy_raster_source(&original);
                                    let mut expected = PixelBuffer::new(width, height);
                                    expected.copy_raster_source(&original);
                                    actual.fill_rect(x, y, w, h, color);
                                    for py in y.max(0)..(y + h).min(height as i32) {
                                        for px in x.max(0)..(x + w).min(width as i32) {
                                            expected.set_pixel(
                                                px as u32, py as u32, color[0], color[1], color[2],
                                                color[3],
                                            );
                                        }
                                    }
                                    assert_eq!(actual.pixels, expected.pixels,
                                    "size={width}x{height} scissor={scissor:?} blend={blend} stencil={stencil} rect={x},{y},{w},{h} color={color:?}");
                                    assert_eq!(actual.stencil, expected.stencil);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
