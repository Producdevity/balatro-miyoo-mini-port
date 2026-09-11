// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct VoucherColumn {
    ux13: f32,
    ux10: f32,
    ux12: f32,
    ux4: f32,
    ux8: f32,
    common_ux4: f32,
    fac2_inner: f32,
}

impl VoucherColumn {
    #[inline]
    pub(super) fn new(ux: f32, t: f32) -> Self {
        Self {
            ux13: 13.0 * ux,
            ux10: 10.0 * ux,
            ux12: 12.0 * ux,
            ux4: 4.0 * ux,
            ux8: 8.0 * ux,
            common_ux4: -4.0 * ux,
            fac2_inner: fast_cos(t * 2.3 + ux * 8.2),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct VoucherRow {
    uy532: f32,
    uy232: f32,
    uy632: f32,
    common_inner_base: f32,
    fac3_inner: f32,
    fac4_inner: f32,
}

impl VoucherRow {
    #[inline]
    pub(super) fn new(uy: f32, t: f32) -> Self {
        Self {
            uy532: 5.32 * uy,
            uy232: 2.32 * uy,
            uy632: 6.32 * uy,
            common_inner_base: t * 5.3 + uy * 4.2,
            fac3_inner: fast_sin(t * 5.3 + uy * 3.2),
            fac4_inner: fast_sin(t * 1.3 + uy * 13.2),
        }
    }
}

#[inline(always)]
pub(super) fn apply_voucher_booster_colour(
    effect: u8,
    fac: f32,
    fac2: f32,
    fac3: f32,
    fac4: f32,
    fac5: f32,
    r: &mut u8,
    g: &mut u8,
    b: &mut u8,
    a: &mut u8,
) {
    let (rf, gf, bf) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
    let low = rf.min(gf.min(bf));
    let high = rf.max(gf.max(bf));
    let delta = if effect == 8 {
        (high - low).max(low * 0.7)
    } else {
        high - low
    };
    let maxfac = (0.6 * (fac.max(fac2.max(fac3.max(0.0))) + (fac + fac2 + fac3 * fac4))).max(0.0);

    let base_r = rf * 0.5 + 0.4;
    let base_g = gf * 0.5 + 0.4;
    let base_b = bf * 0.5 + 0.8;
    let ro = (base_r - delta + delta * maxfac * (0.7 + fac5 * 0.07) - 0.1).clamp(0.0, 1.0);
    let go = (base_g - delta + delta * maxfac * (0.7 - fac5 * 0.17) - 0.1).clamp(0.0, 1.0);
    let bo = (base_b - delta + delta * maxfac * 0.7 - 0.1).clamp(0.0, 1.0);
    *r = (ro * 255.0) as u8;
    *g = (go * 255.0) as u8;
    *b = (bo * 255.0) as u8;

    let alpha_fac = 0.8
        * (1.0_f32.min((0.3 * (low * 0.2).max(delta) + (maxfac * 0.1).max(0.0).min(0.4)).max(0.0)))
            .max(0.0)
        + 0.15 * maxfac * (0.1 + delta);
    *a = ((*a as f32) * alpha_fac).clamp(0.0, 255.0) as u8;
}

#[inline(always)]
pub(super) fn apply_voucher_booster_axis(
    effect: u8,
    sp: &ShaderPre,
    column: VoucherColumn,
    row: VoucherRow,
    r: &mut u8,
    g: &mut u8,
    b: &mut u8,
    a: &mut u8,
) {
    let fast_sin = |x| sp.sin(x);
    let fast_cos = |x| sp.cos(x);
    let common = fast_cos(row.common_inner_base + column.common_ux4);
    let fac = 0.8 + 0.9 * fast_sin(column.ux13 + row.uy532 + sp.t28 * 12.0 + common);
    let fac2 = 0.5 + 0.5 * fast_sin(column.ux10 + row.uy232 + sp.t28 * 5.0 - column.fac2_inner);
    let fac3 = 0.5 + 0.5 * fast_sin(column.ux12 + row.uy632 + sp.t28 * 6.111 + row.fac3_inner);
    let fac4 = 0.5 + 0.5 * fast_sin(column.ux4 + row.uy232 + sp.t28 * 8.111 + row.fac4_inner);
    let fac5 = fast_sin(column.ux8 + row.uy532 + sp.t28 * 12.0 + common);
    apply_voucher_booster_colour(effect, fac, fac2, fac3, fac4, fac5, r, g, b, a);
}

/// Apply spatially-varying card shader effects per-pixel.
/// sp: precomputed time-only values (computed once per sprite, not per pixel)
#[inline(always)]
pub(crate) fn apply_card_shader(
    effect: u8,
    sp: &ShaderPre,
    colour_cache: Option<&mut ColourCache>,
    spatial_sample: Option<[f32; 2]>,
    ux: f32,
    uy: f32,
    _px: f32,
    _py: f32,
    r: &mut u8,
    g: &mut u8,
    b: &mut u8,
    a: &mut u8,
) {
    let fast_sin = |x| sp.sin(x);
    let fast_cos = |x| sp.cos(x);
    match effect {
        1 => {
            // Played: desaturate and darken, halve alpha
            // GLSL: SAT.g *= 0.5, SAT.b *= 0.8, tex.a *= 0.5
            let (h, s, l) = card_hsl(colour_cache, effect, [*r, *g, *b]);
            let (ro, go, bo) = hsl_to_rgb(h, s * 0.5, l * 0.8);
            *r = (ro * 255.0) as u8;
            *g = (go * 255.0) as u8;
            *b = (bo * 255.0) as u8;
            *a = *a >> 1; // * 0.5
        }
        2 => {
            // Debuff: HSL desaturation + reddish tint + diagonal stripe pattern
            // GLSL: blend tex*0.8+0.2*red, convert to HSL, apply stripes via UV
            let (h, _s, l) = card_hsl(colour_cache, effect, [*r, *g, *b]);

            // Diagonal stripe test: (uv.x+uv.y ≈ 1) or ((1-uv.x)+uv.y ≈ 1)
            let stripe_width = 0.1_f32;
            let d1 = (ux + uy - 1.0).abs();
            let d2 = ((1.0 - ux) + uy - 1.0).abs();
            let on_stripe = d1 < stripe_width || d2 < stripe_width;

            if on_stripe {
                // Bright magenta stripe: hue=1 (red), sat=0.7, lightness *= 0.8
                let (ro, go, bo) = hsl_to_rgb(1.0, 0.7, l * 0.8);
                *r = (ro * 255.0).min(255.0) as u8;
                *g = (go * 255.0).min(255.0) as u8;
                *b = (bo * 255.0).min(255.0) as u8;
                // Full alpha on stripe
            } else {
                // Desaturated, dark, 30% alpha
                let (ro, go, bo) = hsl_to_rgb(h, 0.25, l * 0.7);
                *r = (ro * 255.0).min(255.0) as u8;
                *g = (go * 255.0).min(255.0) as u8;
                *b = (bo * 255.0).min(255.0) as u8;
                *a = (*a as u16 * 77 / 256) as u8; // ~30% alpha
            }
        }
        3 => {
            // Foil: radial + angular shimmer in silvery-blue (time-only values precomputed in sp)
            let maxfac = spatial_sample.unwrap_or_else(|| spatial(effect, sp, ux, uy))[0];

            let (rf, gf, bf) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
            let low = rf.min(gf.min(bf));
            let high = rf.max(gf.max(bf));
            // GLSL: delta = min(high, max(0.5, 1.0 - low))
            let delta = high.min((1.0_f32 - low).max(0.5));

            // Foil: silvery-blue shift — red/green get small boost, blue gets strong boost
            let ro = (rf - delta + delta * maxfac * 0.3).clamp(0.0, 1.0);
            let go = (gf - delta + delta * maxfac * 0.3).clamp(0.0, 1.0);
            let bo = (bf + delta * maxfac * 1.9).clamp(0.0, 1.0);
            *r = (ro * 255.0) as u8;
            *g = (go * 255.0) as u8;
            *b = (bo * 255.0) as u8;
            // GLSL: tex.a = min(tex.a, 0.3*tex.a + 0.9*min(0.5, maxfac*0.1))
            // Using min() ensures alpha only decreases, preserving edge pixels at bright shimmer.
            let af = *a as f32 / 255.0;
            let foil_a = af.min(0.3 * af + 0.9 * (maxfac * 0.1).min(0.5));
            *a = (foil_a * 255.0) as u8;
        }
        4 => {
            // Holo: HSL rainbow shift + grid pattern (time-only offsets precomputed in sp)
            let (rf, gf, bf) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
            let (mut h, mut s, mut l) = card_hsl(colour_cache, effect, [*r, *g, *b]);

            let [res, fac] = spatial_sample.unwrap_or_else(|| spatial(effect, sp, ux, uy));

            let low = rf.min(gf.min(bf));
            let high = rf.max(gf.max(bf));
            let delta = 0.2 + 0.3 * (high - low) + 0.1 * high;

            h = h + res + fac;
            s = s * 1.3;
            l = l * 0.6 + 0.4;

            let (hr, hg, hb) = hsl_to_rgb(wrap_hue(h), s.min(1.0), l.min(1.0));
            let ro = (1.0 - delta) * rf + delta * hr * 0.9;
            let go = (1.0 - delta) * gf + delta * hg * 0.8;
            let bo = (1.0 - delta) * bf + delta * hb * 1.2;
            *r = (ro.clamp(0.0, 1.0) * 255.0) as u8;
            *g = (go.clamp(0.0, 1.0) * 255.0) as u8;
            *b = (bo.clamp(0.0, 1.0) * 255.0) as u8;
            if (*a as f32) < 178.5 {
                // < 0.7 * 255
                *a = *a / 3;
            }
        }
        5 => {
            // Polychrome: HSL hue rotation via noise field
            // GLSL: polychrome.x ≈ REAL/28 (slow, drives color sweep)
            //        polychrome.y = REAL (fast, drives field + hue drift)
            let (mut h, s, _l) = card_hsl(colour_cache, effect, [*r, *g, *b]);

            let res = spatial_sample.unwrap_or_else(|| spatial(effect, sp, ux, uy))[0];

            h = h + res + sp.poly_y * 0.04;
            let s_out = s.min(0.6).max(s + 0.5);
            let s_clamped = s_out.min(0.6);

            let (ro, go, bo) = hsl_to_rgb(wrap_hue(h), s_clamped, _l);
            *r = (ro.clamp(0.0, 1.0) * 255.0) as u8;
            *g = (go.clamp(0.0, 1.0) * 255.0) as u8;
            *b = (bo.clamp(0.0, 1.0) * 255.0) as u8;
            if (*a as f32) < 178.5 {
                *a = *a / 3;
            }
        }
        6 => {
            // Negative: HSL-based inversion + blue-green tint
            // GLSL: convert to HSL, invert lightness, shift hue, convert back, add tint
            let (h, s, l) = card_hsl(colour_cache, effect, [*r, *g, *b]);
            // Invert lightness (negative.g != 0 case, which is the normal card state)
            let l_inv = if sp.negative_invert { 1.0 - l } else { l };
            // Shift hue: -h + 0.2
            let h_new = wrap_hue(-h + 0.2);
            let (nr, ng, nb) = hsl_to_rgb(h_new, s, l_inv);
            // Add blue-green tint: + 0.8 * (79/255, 99/255, 103/255)
            *r = ((nr + 0.8 * 79.0 / 255.0).clamp(0.0, 1.0) * 255.0) as u8;
            *g = ((ng + 0.8 * 99.0 / 255.0).clamp(0.0, 1.0) * 255.0) as u8;
            *b = ((nb + 0.8 * 103.0 / 255.0).clamp(0.0, 1.0) * 255.0) as u8;
            if (*a as f32) < 178.5 {
                *a = *a / 3;
            }
        }
        10 => {
            // Negative_shine: multi-component animated sine wave shimmer
            // From negative_shine.fs: 5 sine components create moving light patterns
            let (rf, gf, bf) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
            let low = rf.min(gf.min(bf));
            let high = rf.max(gf.max(bf));
            let delta = high - low - 0.1;

            let t = sp.t28;
            let fac = 0.8
                + 0.9
                    * fast_sin(
                        11.0 * ux + 4.32 * uy + t * 12.0 + fast_cos(t * 5.3 + uy * 4.2 - ux * 4.0),
                    );
            let fac2 =
                0.5 + 0.5 * fast_sin(8.0 * ux + 2.32 * uy + t * 5.0 - fast_cos(t * 2.3 + ux * 8.2));
            let fac3 = 0.5
                + 0.5 * fast_sin(10.0 * ux + 5.32 * uy + t * 6.111 + fast_sin(t * 5.3 + uy * 3.2));
            let fac4 = 0.5
                + 0.5 * fast_sin(3.0 * ux + 2.32 * uy + t * 8.111 + fast_sin(t * 1.3 + uy * 11.2));
            let fac5 = fast_sin(
                0.9 * 16.0 * ux + 5.32 * uy + t * 12.0 + fast_cos(t * 5.3 + uy * 4.2 - ux * 4.0),
            );

            let maxfac =
                (0.7 * (fac.max(fac2.max(fac3.max(0.0))) + (fac + fac2 + fac3 * fac4))).max(0.0);

            // Base: darken original, add blue-ish tint
            let base_r = rf * 0.5 + 0.4;
            let base_g = gf * 0.5 + 0.4;
            let base_b = bf * 0.5 + 0.8;

            let ro = (base_r - delta + delta * maxfac * (0.7 + fac5 * 0.27) - 0.1).clamp(0.0, 1.0);
            let go = (base_g - delta + delta * maxfac * (0.7 - fac5 * 0.27) - 0.1).clamp(0.0, 1.0);
            let bo = (base_b - delta + delta * maxfac * 0.7 - 0.1).clamp(0.0, 1.0);
            *r = (ro * 255.0) as u8;
            *g = (go * 255.0) as u8;
            *b = (bo * 255.0) as u8;

            // Alpha: complex formula from GLSL
            let alpha_fac = 0.5
                * (1.0_f32.min(
                    (0.3 * (low * 0.2).max(delta) + (maxfac * 0.1).max(0.0).min(0.4)).max(0.0),
                ))
                .max(0.0)
                + 0.15 * maxfac * (0.1 + delta);
            *a = ((*a as f32) * alpha_fac).clamp(0.0, 255.0) as u8;
        }
        7 | 8 => {
            let t = sp.t28;
            let common = fast_cos(t * 5.3 + uy * 4.2 - ux * 4.0);
            let fac = 0.8 + 0.9 * fast_sin(13.0 * ux + 5.32 * uy + t * 12.0 + common);
            let fac2 = 0.5
                + 0.5 * fast_sin(10.0 * ux + 2.32 * uy + t * 5.0 - fast_cos(t * 2.3 + ux * 8.2));
            let fac3 = 0.5
                + 0.5 * fast_sin(12.0 * ux + 6.32 * uy + t * 6.111 + fast_sin(t * 5.3 + uy * 3.2));
            let fac4 = 0.5
                + 0.5 * fast_sin(4.0 * ux + 2.32 * uy + t * 8.111 + fast_sin(t * 1.3 + uy * 13.2));
            let fac5 = fast_sin(8.0 * ux + 5.32 * uy + t * 12.0 + common);
            apply_voucher_booster_colour(effect, fac, fac2, fac3, fac4, fac5, r, g, b, a);
        }
        9 => {
            // Hologram: card interior transparent, edges glow with animated cyan light.
            // GLSL hologram.fs: fully opaque pixels → invisible (alpha→0);
            //   semi-transparent edge pixels keep their alpha with cyan-green glow.
            // The draw color in graphics.rs already shifts the image toward cyan-blue
            // (tint ≈ [77, 200, 240, 180/255*original_a]), so *g and *b are already cyan.
            //
            // After the cyan tint, interior pixels have fa ≈ 180 (sa=255 * 180/255).
            // Edge pixels (anti-aliased border) have fa ≈ 70-140 (sa=100-200 * 180/255).
            // Background pixels have fa = 0 (skipped before reaching shader).
            if *a > 165 {
                // Card interior: make fully transparent (hologram ghost effect)
                *a = 0;
            } else if *a > 8 {
                // Card edges/outline: animate with pulsing cyan-green light
                // light_strength from GLSL: 0.4*(0.3*sin(2*holo_g) + 0.6 + 0.3*sin(holo_r*3) + 0.9)
                let light = (0.4 * (0.3 * sp.holo9_sin_g + 0.6 + 0.3 * sp.holo9_sin_r + 0.9))
                    .clamp(0.4_f32, 1.0_f32);
                // Zero red channel, boost green-blue for pure cyan glow
                *r = 0;
                *g = ((*g as f32) * light * 1.3).min(255.0) as u8;
                *b = ((*b as f32) * light).min(255.0) as u8;
                // Keep alpha: don't reduce it further (edges should remain visible)
            }
            // a <= 8: fully transparent background — leave as-is
        }
        11 => {
            // Gold seal: animated golden shine sweep
            // From gold_seal.fs: sine-wave based highlight that sweeps across
            let (rf, gf, bf) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
            let high = rf.max(gf.max(bf));
            let delta = high * 0.5;

            let t = sp.gs_t;
            let fac = 0.3 + fast_sin(ux * 450.0 + sp.gs_sin6t * 180.0 - 700.0 * t)
                - fast_sin(ux * 190.0 + uy * 30.0 + 1080.3 * t);

            let ro = rf.max((1.0 - rf) * delta * fac + rf);
            let go = gf.max((1.0 - gf) * delta * fac + gf);
            let bo = bf.max((1.0 - bf) * delta * fac + bf);
            *r = (ro.clamp(0.0, 1.0) * 255.0) as u8;
            *g = (go.clamp(0.0, 1.0) * 255.0) as u8;
            *b = (bo.clamp(0.0, 1.0) * 255.0) as u8;
        }
        _ => {}
    }
}
