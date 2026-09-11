use super::{
    apply_card_shader, apply_voucher_booster_axis, fast_sin, DissolveParams, PixelBuffer,
    ShaderPre, VoucherColumn, VoucherRow,
};

#[test]
fn multiply_blend_does_not_overflow_channel_products() {
    let mut buffer = PixelBuffer::new(1, 1);
    buffer.blend = 3;
    for alpha in 0..=255_u8 {
        for value in 0..=255_u8 {
            buffer.pixels.copy_from_slice(&[255, 173, 89, 255]);
            buffer.blend_at(0, value, value, value, alpha);
            let expected: Vec<u8> = [255_u64, 173, 89]
                .into_iter()
                .map(|dst| {
                    (dst * u64::from(value) * u64::from(alpha) / 65025
                        + dst * (255 - u64::from(alpha)) / 255) as u8
                })
                .chain([255])
                .collect();
            assert_eq!(buffer.pixels, expected);
        }
    }
}

#[test]
fn precomputed_dissolve_matches_per_pixel_field() {
    for (w, h) in [(71.0, 95.0), (142.0, 190.0), (17.0, 31.0), (95.0, 71.0)] {
        for time in [-31.25, 0.0, 78.125, 9125.5] {
            for d in [0.01_f32, 0.2, 0.5, 0.8, 1.0] {
                let adjusted = (d * d * (3.0 - 2.0 * d)) * 1.02 - 0.01;
                let field = super::DissolveField::new(time, d, adjusted, w, h);
                for y in 0..=95 {
                    for x in 0..=71 {
                        let ux = (x as f32 + 0.125) / 71.0;
                        let uy = (y as f32 + 0.375) / 95.0;
                        let expected = super::dissolve_field(ux, uy, time, d, adjusted, w, h);
                        assert_eq!(field.sample(ux, uy).to_bits(), expected.to_bits());
                    }
                }
            }
        }
    }
}

#[test]
fn spatial_cache_matches_uncached_layers_and_animation() {
    let mut cached = PixelBuffer::new(320, 240);
    let mut reference = PixelBuffer::new(320, 240);
    cached.set_shader_spatial_cache(true);
    reference.set_shader_spatial_cache(false);
    let layers: Vec<Vec<u8>> = (0..2)
        .map(|layer| {
            (0..71 * 95)
                .flat_map(|pixel: u32| {
                    let mut rgba = pixel.wrapping_mul(0x9e3779b1 + layer * 4).to_le_bytes();
                    if (pixel + layer) % 3 == 0 {
                        rgba[3] = 0;
                    }
                    rgba
                })
                .collect()
        })
        .collect();
    for effect in 3..=5 {
        for frame in 0..12 {
            let params = DissolveParams {
                shader_effect: effect,
                shader_inputs: super::CardShaderInputs {
                    phase: 1.25 + frame as f32 * 0.01,
                    clock: 31.0 + frame as f32 * 0.03,
                    seed: 78.125 + (frame % 3) as f32,
                },
                sprite_w: if frame % 2 == 0 { 71.0 } else { 142.0 },
                dissolve: if frame % 3 == 0 { 0.3 } else { 0.0 },
                burn1: [255, 12, 87, 255],
                ..DissolveParams::NONE
            };
            for target in [&mut cached, &mut reference] {
                target.clear(0.12, 0.3, 0.2, 1.0);
                target.filter_linear = frame % 2 == 0;
                for (layer, source) in layers.iter().enumerate() {
                    target.scissor = if layer == 0 {
                        Some((33, 35, 45, 80))
                    } else {
                        None
                    };
                    let scale = if frame % 4 == 0 { 1.5 } else { 0.5 };
                    let sx = if frame % 3 == 0 { -scale } else { scale };
                    let sy = if frame % 5 == 0 { -scale } else { scale };
                    target.draw_image_region(
                        source,
                        71,
                        95,
                        0.0,
                        0.0,
                        71.0,
                        95.0,
                        80.0 + layer as f32,
                        90.0,
                        sx,
                        sy,
                        [255, 193, 217, if layer == 0 { 255 } else { 170 }],
                        false,
                        false,
                        params,
                    );
                }
            }
            assert_eq!(
                cached.pixels, reference.pixels,
                "effect={effect} frame={frame}"
            );
        }
    }
}

#[test]
fn colour_cache_keeps_shader_pixels_and_animation_unchanged() {
    let mut cache = super::ColourCache::new();
    for effect in 0..=11 {
        for time in [-30.25, 0.0, 0.25, 82.25, 9102.5] {
            let shader = ShaderPre::compute(
                effect,
                super::CardShaderInputs {
                    phase: time / 28.0,
                    clock: time,
                    seed: 51.25,
                },
            );
            for step in 0..1024_u32 {
                let colour = step.wrapping_mul(0x9e3779b1).to_le_bytes();
                let ux = (step % 71) as f32 / 71.0;
                let uy = (step % 95) as f32 / 95.0;
                let [mut r, mut g, mut b, mut a] = colour;
                apply_card_shader(
                    effect, &shader, None, None, ux, uy, 0.0, 0.0, &mut r, &mut g, &mut b, &mut a,
                );
                let expected = [r, g, b, a];
                // First conversion may miss; the second must reuse it.
                for _ in 0..2 {
                    let [mut r, mut g, mut b, mut a] = colour;
                    apply_card_shader(
                        effect,
                        &shader,
                        Some(&mut cache),
                        None,
                        ux,
                        uy,
                        0.0,
                        0.0,
                        &mut r,
                        &mut g,
                        &mut b,
                        &mut a,
                    );
                    assert_eq!(
                        [r, g, b, a],
                        expected,
                        "effect={effect} time={time} step={step}"
                    );
                }
            }
        }
    }
}

#[test]
fn colour_cache_allocates_only_for_affected_draws_and_can_be_disabled() {
    let mut target = PixelBuffer::new(1, 1);
    target.set_shader_colour_cache(false);
    target.prepare_shader_colour_cache(4);
    assert!(target.shader_colour_cache.is_none());
    target.set_shader_colour_cache(true);
    for effect in [0, 3, 7, 8, 9, 10, 11] {
        target.prepare_shader_colour_cache(effect);
        assert!(target.shader_colour_cache.is_none());
    }
    target.prepare_shader_colour_cache(4);
    assert!(target.shader_colour_cache.is_some());
    target.set_shader_colour_cache(false);
    assert!(target.shader_colour_cache.is_none());
}

#[test]
fn fast_sine_tracks_standard_sine_across_negative_and_positive_phases() {
    for step in -10_000..=10_000 {
        let phase = step as f32 * 0.03125;
        let error = (fast_sin(phase) - phase.sin()).abs();
        assert!(error <= 0.0008, "phase={phase} error={error}");
    }
}

#[test]
fn axis_aligned_voucher_shader_matches_generic_path() {
    let colours = [
        [0, 0, 0, 0],
        [17, 91, 203, 255],
        [255, 128, 3, 147],
        [231, 231, 231, 255],
    ];
    let coordinates = [(0.0, 0.0), (0.125, 0.875), (0.5, 0.5), (0.9875, 0.025)];

    for effect in [7, 8] {
        for time in [-41.25, 0.0, 3.75, 912.5] {
            let shader = ShaderPre::compute(
                effect,
                super::CardShaderInputs {
                    phase: time / 28.0,
                    clock: time,
                    seed: 51.25,
                },
            );
            for (ux, uy) in coordinates {
                for colour in colours {
                    let [mut gr, mut gg, mut gb, mut ga] = colour;
                    apply_card_shader(
                        effect, &shader, None, None, ux, uy, 0.0, 0.0, &mut gr, &mut gg, &mut gb,
                        &mut ga,
                    );
                    let generic = [gr, gg, gb, ga];

                    let [mut or, mut og, mut ob, mut oa] = colour;
                    apply_voucher_booster_axis(
                        effect,
                        &shader,
                        VoucherColumn::new(ux, shader.t28),
                        VoucherRow::new(uy, shader.t28),
                        &mut or,
                        &mut og,
                        &mut ob,
                        &mut oa,
                    );
                    let optimized = [or, og, ob, oa];

                    for channel in 0..4 {
                        assert!(
                                generic[channel].abs_diff(optimized[channel]) <= 1,
                                "effect={effect} time={time} ux={ux} uy={uy} channel={channel} generic={} optimized={}",
                                generic[channel],
                                optimized[channel]
                            );
                    }
                }
            }
        }
    }
}

#[test]
fn identity_composite_skips_transparent_pixels_and_blends_premultiplied_alpha() {
    let mut target = PixelBuffer::new(3, 2);
    for pixel in target.pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[10, 20, 30, 255]);
    }
    let mut source = vec![0; 3 * 2 * 4];
    source[4..8].copy_from_slice(&[100, 110, 120, 255]);
    source[16..20].copy_from_slice(&[50, 0, 0, 128]);

    target.composite_premultiplied_identity(&source, 3, 2, 0, 0);

    assert_eq!(&target.pixels[0..4], &[10, 20, 30, 255]);
    assert_eq!(&target.pixels[4..8], &[100, 110, 120, 255]);
    assert_eq!(&target.pixels[16..20], &[54, 9, 14, 255]);
}

#[test]
fn translucent_span_matches_pixel_source_over() {
    let color = [213, 57, 149, 173];
    let mut span = PixelBuffer::new(13, 1);
    for (index, pixel) in span.pixels.chunks_exact_mut(4).enumerate() {
        pixel.copy_from_slice(&[
            (index * 17) as u8,
            (index * 7 + 3) as u8,
            (index * 11 + 5) as u8,
            (index * 13 + 19) as u8,
        ]);
    }
    let mut expected = PixelBuffer::new(13, 1);
    expected.pixels.copy_from_slice(&span.pixels);
    for index in 0..13 {
        expected.blend_at(index * 4, color[0], color[1], color[2], color[3]);
    }

    span.fill_rect(0, 0, 13, 1, color);

    assert_eq!(span.pixels, expected.pixels);
}

#[test]
fn nearest_draw_matches_the_reference_sampler() {
    let src_w = 5u32;
    let src_h = 4u32;
    let mut source = vec![0; (src_w * src_h * 4) as usize];
    for (index, pixel) in source.chunks_exact_mut(4).enumerate() {
        pixel.copy_from_slice(&[
            (index * 11) as u8,
            (index * 7 + 3) as u8,
            (index * 5 + 9) as u8,
            if index % 4 == 0 { 96 } else { 255 },
        ]);
    }

    let mut actual = PixelBuffer::new(9, 7);
    let mut expected = PixelBuffer::new(9, 7);
    for pixel in actual.pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[17, 29, 41, 255]);
    }
    expected.pixels.copy_from_slice(&actual.pixels);

    let src_x = 1.0f32;
    let src_y = 0.0f32;
    let src_rw = 4.0f32;
    let src_rh = 4.0f32;
    let dst_x = -1.0f32;
    let dst_y = 1.0f32;
    let sx = 1.75f32;
    let sy = 1.25f32;
    actual.draw_image_region(
        &source,
        src_w,
        src_h,
        src_x,
        src_y,
        src_rw,
        src_rh,
        dst_x,
        dst_y,
        sx,
        sy,
        [255; 4],
        false,
        false,
        DissolveParams::NONE,
    );

    let dst_w = (src_rw * sx.abs()).ceil() as i32;
    let dst_h = (src_rh * sy.abs()).ceil() as i32;
    let dx0 = dst_x as i32;
    let dy0 = dst_y as i32;
    let x_start = (-dx0).max(0);
    let y_start = (-dy0).max(0);
    let x_end = dst_w.min(expected.width as i32 - dx0);
    let y_end = dst_h.min(expected.height as i32 - dy0);
    for py in y_start..y_end {
        let source_y = src_y + py as f32 / sy.abs();
        for px in x_start..x_end {
            let source_x = src_x + px as f32 / sx.abs();
            let source_index = (source_y as u32 * src_w + source_x as u32) as usize * 4;
            let destination_index =
                ((dy0 + py) as u32 * expected.width + (dx0 + px) as u32) as usize * 4;
            let alpha = source[source_index + 3];
            if alpha == 255 {
                expected.pixels[destination_index..destination_index + 4]
                    .copy_from_slice(&source[source_index..source_index + 4]);
            } else if alpha > 0 {
                expected.blend_at(
                    destination_index,
                    source[source_index],
                    source[source_index + 1],
                    source[source_index + 2],
                    alpha,
                );
            }
        }
    }

    assert_eq!(actual.pixels, expected.pixels);
    assert_eq!(
        actual.nearest_columns.len(),
        x_end as usize - x_start as usize
    );
}

#[test]
fn white_text_mask_fast_path_matches_generic_tinting() {
    let source = vec![
        0, 0, 0, 0, 255, 255, 255, 64, 255, 255, 255, 128, 255, 255, 255, 255, 255, 255, 255, 213,
        0, 0, 0, 0, 255, 255, 255, 37, 255, 255, 255, 172,
    ];
    let tint = [83, 149, 227, 191];

    for filter_linear in [false, true] {
        let mut fast = PixelBuffer::new(9, 5);
        let mut generic = PixelBuffer::new(9, 5);
        fast.filter_linear = filter_linear;
        generic.filter_linear = filter_linear;
        for pixel in fast.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[19, 31, 43, 255]);
        }
        generic.pixels.copy_from_slice(&fast.pixels);

        fast.draw_image_region(
            &source,
            4,
            2,
            0.0,
            0.0,
            4.0,
            2.0,
            1.0,
            1.0,
            1.5,
            1.5,
            tint,
            false,
            true,
            DissolveParams::NONE,
        );
        generic.draw_image_region(
            &source,
            4,
            2,
            0.0,
            0.0,
            4.0,
            2.0,
            1.0,
            1.0,
            1.5,
            1.5,
            tint,
            false,
            false,
            DissolveParams::NONE,
        );

        assert_eq!(fast.pixels, generic.pixels);
    }
}
