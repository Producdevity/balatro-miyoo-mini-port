use super::Flame;

const PRIMARY: [f32; 3] = [0.98, 0.18, 0.29];
const SECONDARY: [f32; 3] = [0.15, 0.62, 1.0];

#[cfg(all(target_arch = "arm", feature = "flame-simd"))]
#[test]
fn vector_math_preserves_flame_colours_and_coverage() {
    let mut pixels = 0u64;
    let mut alpha_changes = 0u64;
    let mut colour_changes = 0u64;
    let mut maximum_channel_error = 0u8;
    for time in [0.0, 0.6, 11.37, 92.0, 2500.25, 10000.7] {
        for (intensity, id) in [(0.11, 0.0), (1.2, 1.0), (4.0, 2.0), (10.0, 3.0)] {
            for (width, height) in [(61, 61), (107, 79), (213, 95)] {
                let mut scalar = Flame::new(time, intensity, id, PRIMARY, SECONDARY, true);
                scalar.simd = false;
                let mut vector = Flame::new(time, intensity, id, PRIMARY, SECONDARY, true);
                vector.simd = true;
                for y in 0..height {
                    let uy = y as f32 / height as f32 - 0.5;
                    for x in 0..width {
                        let ux = x as f32 / width as f32 - 0.5;
                        let expected = scalar.pixel(ux, uy);
                        let actual = vector.pixel(ux, uy);
                        pixels += 1;
                        alpha_changes += u64::from(actual[3] != expected[3]);
                        if actual[3] != 0 && expected[3] != 0 {
                            for channel in 0..3 {
                                let error = actual[channel].abs_diff(expected[channel]);
                                colour_changes += u64::from(error != 0);
                                maximum_channel_error = maximum_channel_error.max(error);
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("[flame-math-check] pixels={pixels} alpha_changes={alpha_changes} colour_changes={colour_changes} max_channel_error={maximum_channel_error}");
    assert!(
        alpha_changes * 10000 <= pixels,
        "flame coverage changed by more than 0.01 percent"
    );
    assert!(
        maximum_channel_error <= 1,
        "visible colour changed by more than one byte step"
    );
}

#[test]
fn rectangular_draw_matches_the_previous_pixel_loop() {
    use crate::pixel_buffer::PixelBuffer;
    for rect in [
        [0, 0, 41, 31],
        [-3, -2, 17, 19],
        [5, 2, 20, 13],
        [42, 0, 5, 5],
        [0, 32, 5, 5],
        [0, 0, 0, 5],
        [0, 0, -1, -1],
    ] {
        for blend in 0..6 {
            for cache in [false, true] {
                let mut actual = PixelBuffer::new(41, 31);
                for (index, byte) in actual.pixels.iter_mut().enumerate() {
                    *byte = (index * 19 + 7) as u8;
                }
                actual.blend = blend;
                let mut expected = PixelBuffer::new(41, 31);
                expected.copy_raster_source(&actual);
                let mut flame = Flame::new(11.37, 4.0, 2.0, PRIMARY, SECONDARY, cache);
                flame.draw(&mut actual, rect);
                let mut original = Flame::new(11.37, 4.0, 2.0, PRIMARY, SECONDARY, cache);
                let [x, y, w, h] = rect;
                let x0 = x.max(0) as usize;
                let y0 = y.max(0) as usize;
                let x1 = ((x + w) as usize).min(41);
                let y1 = ((y + h) as usize).min(31);
                let width = if x1 > x0 { x1 - x0 } else { 1 };
                let height = if y1 > y0 { y1 - y0 } else { 1 };
                for py in y0..y1 {
                    let uy = (py - y0) as f32 / height as f32 - 0.5;
                    for px in x0..x1 {
                        let ux = (px - x0) as f32 / width as f32 - 0.5;
                        let [r, g, b, a] = original.pixel(ux, uy);
                        if a > 0 {
                            expected.blend_at((py * 41 + px) * 4, r, g, b, a);
                        }
                    }
                }
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "rect={rect:?} blend={blend} cache={cache}"
                );
            }
        }
    }
}

#[test]
fn rendered_pixels_match_original_scalar() {
    for (time, intensity, id) in [
        (0.0, 0.11, 0.0),
        (0.6, 1.2, 1.0),
        (11.37, 4.0, 2.0),
        (92.0, 10.0, 0.0),
        (2500.25, 8.5, 11.0),
    ] {
        for (width, height) in [(1, 1), (13, 17), (60, 60), (61, 61), (107, 79)] {
            let mut cached = Flame::new(time, intensity, id, PRIMARY, SECONDARY, true);
            let mut direct = Flame::new(time, intensity, id, PRIMARY, SECONDARY, false);
            for y in 0..height {
                let uy = y as f32 / height as f32 - 0.5;
                for x in 0..width {
                    let ux = x as f32 / width as f32 - 0.5;
                    let expected = reference(ux, uy, time, intensity, id, PRIMARY, SECONDARY);
                    assert_eq!(
                        cached.pixel(ux, uy),
                        expected,
                        "cached {width}x{height} ({x},{y})"
                    );
                    assert_eq!(
                        direct.pixel(ux, uy),
                        expected,
                        "direct {width}x{height} ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn quantization_boundaries_and_non_scanline_visits_match() {
    let mut flame = Flame::new(23.14, 6.0, 2.0, PRIMARY, SECONDARY, true);
    for y in [-0.5, -0.1, 0.0, -0.0, 0.11, 0.49, 0.7, -0.7] {
        for cell in -31..=31 {
            let edge = cell as f32 / 60.0;
            for x in [edge, edge - f32::EPSILON, edge + f32::EPSILON, -0.0, 0.0] {
                assert_eq!(
                    flame.pixel(x, y),
                    reference(x, y, 23.14, 6.0, 2.0, PRIMARY, SECONDARY),
                    "({x},{y})"
                );
            }
        }
    }
}

#[test]
fn shared_cell_preserves_continuous_colour_and_edges() {
    let mut flame = Flame::new(6.2, 10.0, 1.0, PRIMARY, SECONDARY, true);
    let mut different_pixels = 0;
    for x in -30..30 {
        for y in -30..30 {
            let ux = (x as f32 + 0.2) / 60.0;
            let uy = (y as f32 + 0.2) / 60.0;
            let p1 = flame.pixel(ux, uy);
            let p2 = flame.pixel(ux + 0.005, uy + 0.005);
            assert_eq!(p1, reference(ux, uy, 6.2, 10.0, 1.0, PRIMARY, SECONDARY));
            assert_eq!(
                p2,
                reference(ux + 0.005, uy + 0.005, 6.2, 10.0, 1.0, PRIMARY, SECONDARY)
            );
            different_pixels += usize::from(p1 != p2);
        }
    }
    assert!(
        different_pixels > 100,
        "fixture must exercise per-pixel changes inside shared cells"
    );
}

// Previous per-pixel implementation, kept separate to catch changes to the
// arithmetic or to the boundary between quantized and continuous coordinates.
fn reference(
    ux: f32,
    uy: f32,
    time: f32,
    intensity: f32, // already clamped to 10.0
    id: f32,
    c1: [f32; 3],
    c2: [f32; 3],
) -> [u8; 4] {
    const PIXEL_SIZE_FAC: f32 = 60.0;

    // Pixelate UV to PIXEL_SIZE_FAC grid
    let floored_x = (ux * PIXEL_SIZE_FAC).floor() / PIXEL_SIZE_FAC;
    let floored_y = (uy * PIXEL_SIZE_FAC).floor() / PIXEL_SIZE_FAC;

    // Small wavering wobble
    let wobble =
        0.01 * (-1.123 * floored_x + 0.2 * time).sin() * (5.3332 * floored_y + time * 0.931).cos();
    let usc_x = floored_x + floored_x * wobble;
    let usc_y = floored_y + floored_y * wobble;

    // Upward-scrolling offset (gives fire the rising motion)
    let flame_up_y = (4.0 * time).rem_euclid(10000.0) - 5000.0 + (1.781 * id).rem_euclid(1000.0);

    let scale_fac = 7.5 + 3.0 / (2.0 + 2.0 * intensity);

    let mut sv_x = usc_x * scale_fac;
    let mut sv_y = usc_y * scale_fac + flame_up_y;

    let speed = (20.781 * id).rem_euclid(100.0) + (time + id).sin() * (time * 0.151 + id).cos();

    let mut sv2_x = 0.0f32;
    let mut sv2_y = 0.0f32;

    // 5-iteration turbulence loop (matches GLSL exactly)
    // Note: mod(float(i), 2.) > 1. is never true for i in 0..4, so sign = +1 always
    for _ in 0..5 {
        let len_sv = (sv_x * sv_x + sv_y * sv_y).sqrt();
        let noise = 0.3 * ((len_sv * 0.411).cos() + 0.3344 * len_sv.sin() - 0.23 * len_sv.cos());
        // GLSL simultaneous vec2 update using old sv2 values and sv2.yx swizzle
        let new_sv2_x = sv2_x + sv_x + 0.05 * sv2_y + noise;
        let new_sv2_y = sv2_y + sv_y + 0.05 * sv2_x + noise;
        sv2_x = new_sv2_x;
        sv2_y = new_sv2_y;
        // sv update uses the new sv2
        sv_x += 0.5 * (sv2_y.cos() + speed * 0.0812).cos() * (3.22 + sv2_x - speed * 0.1531).sin();
        sv_y += 0.5
            * (-sv2_x * 1.21222 + 0.113785 * speed).sin()
            * (sv2_y * 0.91213 - 0.13582 * speed).cos();
    }

    // Smoke density: distance of sv from the upward offset, normalized back to UV space
    let dist_x = (sv_x) / scale_fac * 5.0;
    let dist_y = (sv_y - flame_up_y) / scale_fac * 5.0;
    let len_dist = (dist_x * dist_x + dist_y * dist_y).sqrt();
    let len_usc = (usc_x * usc_x + usc_y * usc_y).sqrt();

    let mut smoke_res =
        (len_dist + 0.1 * (len_usc - 0.5)).max(0.0) * (2.0 / (2.0 + intensity * 0.2));

    // Fade out toward top of sprite (usc_y = -0.5 → large term, usc_y = 0.5 → 0)
    let fade_top =
        (2.0 - 0.3 * intensity).max(0.0) * (2.0 * (usc_y - 0.5) * (usc_y - 0.5)).max(0.0);
    smoke_res += fade_top;

    // Clip beyond horizontal edges
    if ux.abs() > 0.4 {
        smoke_res += 10.0 * (ux.abs() - 0.4);
    }

    // Small dip at bottom center: punch through smoke if inside the oval near (0, 0.1)
    let adj_x = ux * 0.19;
    let adj_y = uy - 0.1;
    let len_adj = (adj_x * adj_x + adj_y * adj_y).sqrt();
    if len_adj < (0.1f32).min(intensity * 0.5) && smoke_res > 1.0 {
        smoke_res += (intensity * 10.0).min(8.5) * (len_adj - 0.1);
    }

    if smoke_res > 1.0 {
        return [0, 0, 0, 0];
    }

    // Color: mostly c1, blend toward c2 in the upper portion (uy < 0.12)
    let mut r = c1[0];
    let mut g = c1[1];
    let mut b = c1[2];
    if uy < 0.12 {
        let diff = 0.12 - uy;
        r = c1[0] * (1.0 - 0.5 * diff) + 2.5 * diff * c2[0];
        g = c1[1] * (1.0 - 0.5 * diff) + 2.5 * diff * c2[1];
        b = c1[2] * (1.0 - 0.5 * diff) + 2.5 * diff * c2[2];
        let mod_f = (-2.0 + 0.5 * intensity * smoke_res) * diff;
        r = (r + r * mod_f).max(0.0);
        g = (g + g * mod_f).max(0.0);
        b = (b + b * mod_f).max(0.0);
    }

    [
        (r * 255.0).clamp(0.0, 255.0) as u8,
        (g * 255.0).clamp(0.0, 255.0) as u8,
        (b * 255.0).clamp(0.0, 255.0) as u8,
        255,
    ]
}
