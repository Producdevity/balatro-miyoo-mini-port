use super::*;
use crate::card_effects::CardShaderInputs;
use crate::neon_sprite::Sprite;
use crate::pixel_buffer::PixelBuffer;

unsafe extern "C" {
    fn balatro_shaded_sprite_pair(
        effect: u32,
        base: *const Sprite,
        overlay: *const Sprite,
        uniforms: *const Uniforms,
        source: *const u8,
        overlay_source: *const u8,
        target: *mut u8,
        cache: *mut std::ffi::c_void,
        fill: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut Samples, usize),
    );
}

#[cfg(feature = "layer-pairs")]
#[test]
fn pair_api_rejects_unsupported_state_without_writing() {
    use crate::pixel_buffer::{DissolveParams, StencilCompare};
    let source = vec![255; 17 * 13 * 4];
    for case in 0..7 {
        let mut target = PixelBuffer::new(43, 31);
        target.pixels.fill(127);
        let before = target.pixels.clone();
        let mut tint = [255; 4];
        let mut params = DissolveParams {
            shader_effect: 4,
            ..DissolveParams::NONE
        };
        let mut overlay_region = [0.0, 0.0, 17.0, 13.0];
        match case {
            0 => target.filter_linear = true,
            1 => target.stencil_write_mode = true,
            2 => target.stencil_compare = StencilCompare::Equal,
            3 => tint[3] = 254,
            4 => params.dissolve = 0.001,
            5 => params.shader_effect = 3,
            _ => overlay_region[2] = 16.0,
        }
        assert!(!target.draw_card_layer_pair(
            &source,
            17,
            [0.0, 0.0, 17.0, 13.0],
            &source,
            17,
            overlay_region,
            [0, 0, 43, 31],
            [0.4, 0.02, 0.0, -0.02, 0.4, 0.0],
            tint,
            false,
            params,
            None,
        ));
        assert_eq!(target.pixels, before, "case={case}");
    }
}

#[test]
fn opaque_layer_pair_matches_separate_draws() {
    let mut random = 0x730ae811_u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    };
    let mut source = vec![0; 97 * 73 * 4 + 1];
    let mut overlay = vec![0; 83 * 91 * 4 + 1];
    for pixels in [&mut source, &mut overlay] {
        for pixel in pixels[1..].chunks_exact_mut(4) {
            let value = next();
            pixel.copy_from_slice(&value.to_le_bytes());
            pixel[3] = if value % 3 == 0 { 0 } else { 255 };
        }
    }
    for sample in 0..2400 {
        let effect = if sample % 2 == 0 { 4 } else { 5 };
        let mut actual = PixelBuffer::new(43, 31);
        actual.pixels.fill(127);
        actual.pixels.extend_from_slice(&[137; 16]);
        actual.blend = if sample % 3 == 0 { 4 } else { 0 };
        let mut expected = actual.pixels.clone();
        let inverse = [
            (next() as i32 % 1024) as f32 / 1024.0,
            (next() as i32 % 512) as f32 / 1024.0,
            (next() as i32 % 256) as f32 / 128.0,
            (next() as i32 % 512) as f32 / 1024.0,
            (next() as i32 % 1024) as f32 / 1024.0,
            (next() as i32 % 256) as f32 / 128.0,
        ];
        let bounds = [(next() % 10) as i32, (next() % 10) as i32, 43, 31];
        let mut tint = next().to_le_bytes();
        tint[3] = 255;
        let replace = sample % 5 == 0;
        let base = Sprite::new(
            &actual,
            &source[1..],
            97,
            [59.0, 37.0, 17.0, 13.0],
            bounds,
            inverse,
            tint,
            false,
            replace,
        )
        .unwrap();
        let top = Sprite::new(
            &actual,
            &overlay[1..],
            83,
            [11.0, 63.0, 17.0, 13.0],
            bounds,
            inverse,
            tint,
            false,
            replace,
        )
        .unwrap();
        let inputs = CardShaderInputs {
            phase: (next() % 10000) as f32 / 731.0,
            clock: (next() % 10000) as f32 / 83.0,
            seed: (next() % 10000) as f32 / 331.0,
        };
        let batch = ShaderBatch::create(effect, &ShaderPre::compute(effect, inputs));
        let mut cache = Some(ColourCache::new());
        let mut cache = cache.as_mut();
        unsafe {
            raster(
                effect,
                &base,
                &batch.uniforms,
                source.as_ptr().add(1),
                expected.as_mut_ptr(),
                None,
            );
            raster(
                effect,
                &top,
                &batch.uniforms,
                overlay.as_ptr().add(1),
                expected.as_mut_ptr(),
                None,
            );
            balatro_shaded_sprite_pair(
                effect as u32,
                &base,
                &top,
                &batch.uniforms,
                source.as_ptr().add(1),
                overlay.as_ptr().add(1),
                actual.pixels.as_mut_ptr(),
                (&mut cache as *mut Option<&mut ColourCache>).cast(),
                fill_hsl,
            );
        }
        assert_eq!(actual.pixels, expected, "sample={sample} effect={effect}");
        #[cfg(feature = "layer-pairs")]
        {
            actual.pixels.fill(127);
            let end = actual.pixels.len();
            actual.pixels[end - 16..].fill(137);
            assert!(worker::draw_pair(
                effect,
                &base,
                &top,
                &batch.uniforms,
                &source[1..],
                &overlay[1..],
                &mut actual.pixels,
                None,
            ));
            assert_eq!(
                actual.pixels, expected,
                "worker sample={sample} effect={effect}"
            );
            let hidden = [
                (next() % 65) as i32 - 15,
                (next() % 51) as i32 - 15,
                (next() % 65) as i32 - 15,
                (next() % 51) as i32 - 15,
            ];
            let mut clipped = expected.clone();
            for y in hidden[1].max(0)..hidden[3].min(31) {
                for x in hidden[0].max(0)..hidden[2].min(43) {
                    let index = (y as usize * 43 + x as usize) * 4;
                    clipped[index..index + 4].fill(127);
                }
            }
            actual.pixels.fill(127);
            actual.pixels[end - 16..].fill(137);
            assert!(batch.draw_pair(
                &mut actual,
                &source[1..],
                97,
                [59.0, 37.0, 17.0, 13.0],
                &overlay[1..],
                83,
                [11.0, 63.0, 17.0, 13.0],
                bounds,
                inverse,
                tint,
                replace,
                Some(hidden),
            ));
            assert_eq!(
                actual.pixels, clipped,
                "clipped sample={sample} effect={effect}"
            );
        }
    }
}
