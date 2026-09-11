use sprite_to_text::pixel_buffer::{CardShaderInputs, DissolveParams, PixelBuffer};
use sprite_to_text::prepared_card::PreparedCard;
use std::time::{Duration, Instant};

fn main() {
    let frames: usize = std::env::args()
        .nth(1)
        .map(|value| value.parse().expect("frame count must be an integer"))
        .unwrap_or(120);
    assert!(frames > 0);
    let layers: usize = std::env::var("CARD_LAYERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    assert!((1..=2).contains(&layers));
    let scale: f32 = std::env::var("CARD_SCALE")
        .unwrap_or_else(|_| "0.5".into())
        .parse()
        .expect("card scale must be a number");
    assert!(scale == 0.5 || scale == 1.5);
    let large = scale == 1.5;
    let mut source = vec![0u8; 71 * 95 * 4];
    for y in 0..95 {
        for x in 0..71 {
            let offset = (y * 71 + x) * 4;
            source[offset..offset + 4].copy_from_slice(&[
                ((x * 17 + y * 3) % 256) as u8,
                ((x * 5 + y * 11) % 256) as u8,
                ((x * 7 + y * 19) % 256) as u8,
                if x < 2 || y < 2 { 0 } else { 255 },
            ]);
        }
    }
    if let Some(path) = std::env::args().nth(2) {
        source = std::fs::read(path).expect("cannot read RGBA card image");
        assert_eq!(
            source.len(),
            71 * 95 * 4,
            "card image must be 71x95 RGBA bytes"
        );
    }
    let mut target = if large {
        PixelBuffer::new(640, 480)
    } else {
        PixelBuffer::new(320, 240)
    };
    let mut front = source.clone();
    for (index, pixel) in front.chunks_exact_mut(4).enumerate() {
        if index % 3 != 0 {
            pixel[3] = 0;
        }
        pixel[0] = 255 - pixel[0];
    }
    println!("effect,transform,frames,microseconds_per_frame,checksum");
    for effect in 0..=11 {
        let prepared: Vec<_> = [&source, &front]
            .into_iter()
            .map(|image| {
                if std::env::var("CARD_PREPARED").as_deref() == Ok("1") {
                    PreparedCard::new(image, 71, [0, 0, 71, 95], effect, true)
                } else {
                    None
                }
            })
            .collect();
        for rotated in [false, true] {
            #[cfg(feature = "shader-cache-stats")]
            let (start_hits, start_misses) = target.shader_colour_cache_stats();
            let mut elapsed = Duration::ZERO;
            let mut checksum = 0xcbf29ce484222325u64;
            for frame in 0..frames {
                target.clear(0.12, 0.2, 0.16, 1.0);
                let mut params = DissolveParams {
                    shader_effect: effect,
                    shader_inputs: CardShaderInputs {
                        phase: (frame as f32 * 0.0625 + 30.0) / 28.0,
                        clock: frame as f32 * 0.0625 + 30.0,
                        seed: 153.125,
                    },
                    ..DissolveParams::NONE
                };
                let started = Instant::now();
                for card in 0..8 {
                    params.shader_inputs.seed = 153.125 + card as f32 * 71.375;
                    let x = 12.0 + (card % 4) as f32 * if large { 144.0 } else { 74.0 };
                    let y = 40.0 + (card / 4) as f32 * if large { 170.0 } else { 70.0 };
                    for (layer, source) in [&source, &front].into_iter().take(layers).enumerate() {
                        if rotated {
                            let angle = 0.08_f32;
                            let c = angle.cos() / scale;
                            let s = angle.sin() / scale;
                            let bounds = if large {
                                [x as i32 - 13, y as i32, x as i32 + 107, y as i32 + 152]
                            } else {
                                [x as i32 - 5, y as i32, x as i32 + 36, y as i32 + 51]
                            };
                            if let Some(card) = &prepared[layer] {
                                card.draw(
                                    &mut target,
                                    bounds,
                                    [c, s, -c * x - s * y, -s, c, s * x - c * y],
                                    false,
                                );
                                continue;
                            }
                            target.draw_image_region_transformed(
                                &source,
                                71,
                                0.0,
                                0.0,
                                71.0,
                                95.0,
                                (bounds[0], bounds[1], bounds[2], bounds[3]),
                                [c, s, -c * x - s * y, -s, c, s * x - c * y],
                                [255; 4],
                                false,
                                false,
                                params,
                            );
                        } else {
                            target.draw_image_region(
                                &source, 71, 95, 0.0, 0.0, 71.0, 95.0, x, y, scale, scale,
                                [255; 4], false, false, params,
                            );
                        }
                    }
                }
                elapsed += started.elapsed();
                for &value in &target.pixels {
                    checksum = (checksum ^ u64::from(value)).wrapping_mul(0x100000001b3);
                }
            }
            println!(
                "{effect},{},{frames},{:.1},{checksum:016x}",
                if rotated { "rotated" } else { "axis" },
                elapsed.as_secs_f64() * 1_000_000.0 / frames as f64
            );
            #[cfg(feature = "shader-cache-stats")]
            {
                let (hits, misses) = target.shader_colour_cache_stats();
                eprintln!(
                    "effect={effect} rotated={rotated} hits={} misses={}",
                    hits - start_hits,
                    misses - start_misses
                );
            }
        }
    }
}
