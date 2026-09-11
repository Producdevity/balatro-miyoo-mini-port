use super::*;
use crate::card_effects::CardShaderInputs;
use crate::pixel_buffer::{PixelBuffer, StencilCompare};

fn reference(draw: impl FnOnce()) {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FUSED_REFERENCE.set(self.0);
        }
    }
    let _restore = Restore(FUSED_REFERENCE.replace(true));
    draw();
}

#[test]
fn fused_sampling_matches_separate_loops_for_all_effects() {
    let mut random = 0x783a29e1_u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    };
    let storage: Vec<u8> = (0..17 * 13 * 4 + 1).map(|_| next() as u8).collect();
    let source = &storage[1..];
    for effect in 1..=6 {
        for iteration in 0..800 {
            let inverse = [
                (next() as i32 % 8192) as f32 / 4096.0,
                (next() as i32 % 1024) as f32 / 4096.0,
                (next() as i32 % 2048) as f32 / 128.0,
                (next() as i32 % 1024) as f32 / 4096.0,
                (next() as i32 % 8192) as f32 / 4096.0,
                (next() as i32 % 2048) as f32 / 128.0,
            ];
            let region = [
                (next() as i32 % 256) as f32 / 64.0,
                (next() as i32 % 256) as f32 / 64.0,
                (next() % 2048 + 1) as f32 / 128.0,
                (next() % 2048 + 1) as f32 / 128.0,
            ];
            let first = (next() % 43) as i32;
            let bounds = [first, 0, first + (next() % (44 - first as u32)) as i32, 31];
            let tint = if iteration % 3 == 0 {
                [255; 4]
            } else {
                next().to_le_bytes()
            };
            let replace = next() & 1 != 0;
            let white_mask = next() & 1 != 0;
            let mut actual = PixelBuffer::new(43, 31);
            actual.clear(0.2, 0.4, 0.7, 0.5);
            actual.blend = if next() & 1 == 0 { 0 } else { 4 };
            let mut expected = PixelBuffer::new(43, 31);
            expected.copy_raster_source(&actual);
            actual.pixels.extend_from_slice(&[17, 33, 65, 129]);
            expected.pixels.extend_from_slice(&[17, 33, 65, 129]);
            let mut pre = ShaderPre::compute(
                effect,
                CardShaderInputs {
                    phase: iteration as f32 / 28.0,
                    clock: iteration as f32,
                    seed: 987.125,
                },
            );
            pre.texture_size = match iteration % 5 {
                0 => [0.5, 1.0],
                1 => [255.0, 255.75],
                2 => [256.0, 1024.0],
                3 => [17.5, 31.25],
                _ => [71.0, 95.0],
            };
            let mut batch = ShaderBatch::create(effect, &pre);
            reference(|| {
                expected.draw_nearest_affine::<true>(
                    source,
                    17,
                    region,
                    bounds,
                    inverse,
                    tint,
                    white_mask,
                    replace,
                    Some(&mut batch),
                )
            });
            let fused = ShaderBatch::create(effect, &pre);
            assert!(fused.draw_affine(
                &mut actual,
                source,
                17,
                region,
                bounds,
                inverse,
                tint,
                white_mask,
                replace
            ));
            assert_eq!(
                actual.pixels, expected.pixels,
                "effect={effect} iteration={iteration} region={region:?} inverse={inverse:?}"
            );
        }
    }
}

#[test]
fn fused_blending_matches_every_alpha_with_transparent_gaps_and_tails() {
    let source: Vec<u8> = (0..256_u32)
        .flat_map(|n| [(n * 73) as u8, (n * 41) as u8, 211, n as u8])
        .collect();
    for effect in 1..=6 {
        for blend in [0, 4] {
            for replace in [false, true] {
                for white_mask in [false, true] {
                    for tint in [[255; 4], [97, 173, 37, 129], [19, 67, 197, 0]] {
                        let mut actual = PixelBuffer::new(33, 9);
                        actual.clear(0.3, 0.7, 0.2, 0.6);
                        actual.blend = blend;
                        let mut expected = PixelBuffer::new(33, 9);
                        expected.copy_raster_source(&actual);
                        let pre = ShaderPre::compute(
                            effect,
                            CardShaderInputs {
                                phase: 0.37,
                                clock: 3.5,
                                seed: 987.125,
                            },
                        );
                        let region = [0.0, 0.0, 32.0, 8.0];
                        let bounds = [0, 0, 33, 9];
                        let inverse = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
                        let mut batch = ShaderBatch::create(effect, &pre);
                        reference(|| {
                            expected.draw_nearest_affine::<true>(
                                &source,
                                32,
                                region,
                                bounds,
                                inverse,
                                tint,
                                white_mask,
                                replace,
                                Some(&mut batch),
                            )
                        });
                        assert!(batch.draw_affine(
                            &mut actual,
                            &source,
                            32,
                            region,
                            bounds,
                            inverse,
                            tint,
                            white_mask,
                            replace
                        ));
                        assert_eq!(actual.pixels, expected.pixels,
                            "effect={effect} blend={blend} replace={replace} white={white_mask} tint={tint:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn fused_draw_rejects_unsupported_buffers_without_writing() {
    let source = [255; 4 * 4 * 4];
    let pre = ShaderPre::compute(4, CardShaderInputs::default());
    let batch = ShaderBatch::create(4, &pre);
    let inverse = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    for case in 0..5 {
        let mut target = PixelBuffer::new(4, 4);
        let mut bounds = [0, 0, 4, 4];
        let mut matrix = inverse;
        let mut width = 4;
        match case {
            0 => target.stencil_compare = StencilCompare::Equal,
            1 => target.blend = 3,
            2 => bounds[2] = 5,
            3 => matrix[0] = f32::NAN,
            _ => width = 0,
        }
        let previous = target.pixels.clone();
        assert!(!batch.draw_affine(
            &mut target,
            &source,
            width,
            [0.0, 0.0, 4.0, 4.0],
            bounds,
            matrix,
            [255; 4],
            false,
            false
        ));
        assert_eq!(target.pixels, previous);
    }
}
