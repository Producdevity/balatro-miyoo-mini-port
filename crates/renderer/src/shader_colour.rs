// Keep the rounding of ((hue % 1.0) + 1.0) % 1.0 without calling fmodf.
#[inline(always)]
pub(crate) fn wrap_hue(hue: f32) -> f32 {
    let magnitude = hue.abs();
    let fraction = if magnitude < 1.0 {
        hue
    } else if magnitude < 8_388_608.0 {
        hue - (hue as i32) as f32
    } else if hue.is_finite() {
        // Every finite f32 at this magnitude is already an integer.
        return 0.0;
    } else {
        return f32::NAN;
    };
    let shifted = fraction + 1.0;
    if shifted >= 2.0 {
        0.0
    } else if shifted >= 1.0 {
        shifted - 1.0
    } else {
        shifted
    }
}

// RGB and HSL components use the 0..1 range, as in the card shaders.
#[inline(always)]
pub(crate) fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let low = r.min(g.min(b));
    let high = r.max(g.max(b));
    let delta = high - low;
    let sum = high + low;
    let l = sum * 0.5;
    if delta < 0.001 {
        return (0.0, 0.0, l);
    }
    let s = if l < 0.5 {
        delta / sum
    } else {
        delta / (2.0 - sum)
    };
    let h = if high == r {
        (g - b) / delta
    } else if high == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (wrap_hue(h / 6.0), s, l)
}

#[inline(always)]
pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 0.0001 {
        return (l, l, l);
    }
    let t = if l < 0.5 { s * l + l } else { -s * l + (s + l) };
    let sv = 2.0 * l - t;
    let hue_comp = |hp: f32| -> f32 {
        let hs = wrap_hue(hp) * 6.0;
        if hs < 1.0 {
            (t - sv) * hs + sv
        } else if hs < 3.0 {
            t
        } else if hs < 4.0 {
            (t - sv) * (4.0 - hs) + sv
        } else {
            sv
        }
    };
    (
        hue_comp(h + 1.0 / 3.0),
        hue_comp(h),
        hue_comp(h - 1.0 / 3.0),
    )
}

#[cfg(test)]
mod tests {
    use super::wrap_hue;

    fn compare(hue: f32) {
        let expected = ((hue % 1.0) + 1.0) % 1.0;
        let actual = wrap_hue(hue);
        if expected.is_nan() {
            assert!(actual.is_nan());
        } else {
            assert_eq!(expected.to_bits(), actual.to_bits(), "hue={hue:?}");
        }
    }

    #[test]
    fn hue_wrap_preserves_float_rounding_at_boundaries() {
        for value in [0.0_f32, 0.5, 1.0, 2.0, 3.0, 8_388_608.0, f32::MAX] {
            for offset in -8_i64..=8 {
                let bits = (i64::from(value.to_bits()) + offset).clamp(0, u32::MAX as i64);
                let hue = f32::from_bits(bits as u32);
                compare(hue);
                compare(-hue);
            }
        }
        compare(f32::INFINITY);
        compare(f32::NEG_INFINITY);
        compare(f32::NAN);
    }

    #[test]
    fn hue_wrap_matches_remainder_across_float_exponents() {
        let mut bits = 0x937dea51_u32;
        for _ in 0..1_000_000 {
            bits ^= bits << 13;
            bits ^= bits >> 17;
            bits ^= bits << 5;
            compare(f32::from_bits(bits));
        }
        for step in -100_000..=100_000 {
            compare(step as f32 / 8192.0);
        }
    }
}
