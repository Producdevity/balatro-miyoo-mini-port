pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("BALATRO_AFFINE_SPANS").as_deref() != Ok("0"));
    *ENABLED
}

// Float evaluation is monotone along a scanline. Solve each edge in double
// precision, then check the neighbouring pixels with the rasterizer's exact
// float operations. The correction retains its rounding at the card boundary.
fn crossing(start: i32, end: i32, slope: f32, reciprocal: f64, base: f32, edge: f32) -> i32 {
    let estimate = (f64::from(edge) - f64::from(base)) * reciprocal - 0.5;
    let mut x = (estimate as i32).clamp(start, end);
    let passed = |x: i32| {
        let value = slope * (x as f32 + 0.5) + base;
        if slope > 0.0 {
            value >= edge
        } else {
            value < edge
        }
    };
    while x > start && passed(x - 1) {
        x -= 1;
    }
    while x < end && !passed(x) {
        x += 1;
    }
    x
}

pub(crate) struct Axis {
    slope: f32,
    reciprocal: f64,
    limit: f32,
}

impl Axis {
    pub(crate) fn new(slope: f32, limit: f32) -> Self {
        Self {
            slope,
            reciprocal: if slope == 0.0 {
                0.0
            } else {
                f64::from(slope).recip()
            },
            limit,
        }
    }

    pub(crate) fn clip(&self, start: i32, end: i32, base: f32) -> (i32, i32) {
        let Self {
            slope,
            reciprocal,
            limit,
        } = *self;
        if start >= end {
            return (start, start);
        }
        if slope == 0.0 {
            return (
                start,
                if base >= 0.0 && base < limit {
                    end
                } else {
                    start
                },
            );
        }
        let (entry, exit) = if slope > 0.0 {
            (0.0, limit)
        } else {
            (limit, 0.0)
        };
        let start = crossing(start, end, slope, reciprocal, base, entry);
        (start, crossing(start, end, slope, reciprocal, base, exit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare(start: i32, end: i32, slope: f32, base: f32, limit: f32) {
        let (first, last) = Axis::new(slope, limit).clip(start, end, base);
        for x in start..end {
            let value = slope * (x as f32 + 0.5) + base;
            assert_eq!(
                (first..last).contains(&x),
                value >= 0.0 && value < limit,
                "x={x} span={first}..{last} slope={slope} base={base} limit={limit}"
            );
        }
    }

    #[test]
    fn span_matches_per_pixel_checks_at_float_boundaries() {
        for slope in [
            -2.3_f32,
            -0.7,
            -0.01,
            -f32::MIN_POSITIVE,
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            0.01,
            0.7,
            2.3,
        ] {
            for limit in [0.01, 1.0, 71.0, 95.0, 142.0, 1024.0] {
                for x in [0, 1, 239, 320, 639] {
                    for edge in [0.0, limit] {
                        let base = edge - slope * (x as f32 + 0.5);
                        for candidate in [base.next_down(), base, base.next_up()] {
                            compare(0, 640, slope, candidate, limit);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn span_matches_random_transforms_and_clips() {
        let mut random = 0x879321bc_u32;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random
        };
        for _ in 0..20_000 {
            let start = (next() % 640) as i32;
            let end = start + (next() % (641 - start as u32)) as i32;
            let slope = (next() as i32 % 30000) as f32 / 1024.0;
            let base = (next() as i32 % 1_000_000) as f32 / 1024.0;
            let limit = (next() % 4096 + 1) as f32 / 4.0;
            compare(start, end, slope, base, limit);
        }
    }
}
