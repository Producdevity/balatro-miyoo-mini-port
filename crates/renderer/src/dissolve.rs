use crate::card_effects::TrigLookup;

pub(crate) struct DissolveField {
    trig: TrigLookup,
    sprite_w: f32,
    sprite_h: f32,
    max_dim: f32,
    offsets: [f32; 6],
    dissolve: f32,
    adjusted: f32,
}

impl DissolveField {
    pub(crate) fn new(time: f32, dissolve: f32, adjusted: f32, w: f32, h: f32) -> Self {
        let trig = TrigLookup::new();
        let t = time * 10.0 + 2003.0;
        Self {
            trig,
            sprite_w: w,
            sprite_h: h,
            max_dim: w.max(h),
            offsets: [
                50.0 * trig.sin(-t / 143.634),
                50.0 * trig.cos(-t / 99.4324),
                50.0 * trig.cos(t / 53.1532),
                50.0 * trig.cos(t / 61.4532),
                50.0 * trig.sin(-t / 87.53218),
                50.0 * trig.sin(-t / 49.0),
            ],
            dissolve,
            adjusted,
        }
    }

    #[inline(always)]
    pub(crate) fn sample(&self, ux: f32, uy: f32) -> f32 {
        let floored_x = (ux * self.sprite_w).floor() / self.max_dim;
        let floored_y = (uy * self.sprite_h).floor() / self.max_dim;
        self.finish(floored_x, floored_y, self.noise(floored_x, floored_y))
    }

    #[inline(always)]
    pub(crate) fn sample_cached(&self, ux: f32, uy: f32, grid: &mut NoiseGrid) -> f32 {
        let x = (ux * self.sprite_w).floor();
        let y = (uy * self.sprite_h).floor();
        if x < 0.0 || y < 0.0 || x >= grid.width as f32 || y >= grid.height as f32 {
            return self.sample(ux, uy);
        }
        let floored_x = x / self.max_dim;
        let floored_y = y / self.max_dim;
        let value = &mut grid.samples[y as usize * grid.width + x as usize];
        if value.is_nan() {
            *value = self.noise(floored_x, floored_y);
        }
        self.finish(floored_x, floored_y, *value)
    }

    #[inline(always)]
    fn noise(&self, floored_x: f32, floored_y: f32) -> f32 {
        let usc_x = (floored_x - 0.5) * 2.3 * self.max_dim;
        let usc_y = (floored_y - 0.5) * 2.3 * self.max_dim;
        let [a, b, c, d, e, f] = self.offsets;
        let (f1x, f1y) = (usc_x + a, usc_y + b);
        let (f2x, f2y) = (usc_x + c, usc_y + d);
        let (f3x, f3y) = (usc_x + e, usc_y + f);
        let len1 = (f1x * f1x + f1y * f1y).sqrt();
        let len2 = (f2x * f2x + f2y * f2y).sqrt();
        let len3 = (f3x * f3x + f3y * f3y).sqrt();
        (1.0 + self.trig.cos(len1 / 19.483)
            + self.trig.sin(len2 / 33.155) * self.trig.cos(f2y / 15.73)
            + self.trig.cos(len3 / 27.193) * self.trig.sin(f3x / 21.92))
            / 2.0
    }

    #[inline(always)]
    fn finish(&self, floored_x: f32, floored_y: f32, field: f32) -> f32 {
        let d = self.dissolve;
        0.5 + 0.5
            * self
                .trig
                .cos(self.adjusted / 82.612 + (field - 0.5) * std::f32::consts::PI)
            - if floored_x > 0.8 {
                (floored_x - 0.8) * (5.0 + 5.0 * d) * d
            } else {
                0.0
            }
            - if floored_y > 0.8 {
                (floored_y - 0.8) * (5.0 + 5.0 * d) * d
            } else {
                0.0
            }
            - if floored_x < 0.2 {
                (0.2 - floored_x) * (5.0 + 5.0 * d) * d
            } else {
                0.0
            }
            - if floored_y < 0.2 {
                (0.2 - floored_y) * (5.0 + 5.0 * d) * d
            } else {
                0.0
            }
    }
}

const MAX_GRIDS: usize = 12;
const MAX_SAMPLES: usize = 8192;

pub(crate) struct NoiseGrid {
    key: [u32; 3],
    width: usize,
    height: usize,
    samples: Vec<f32>,
}

#[derive(Default)]
pub(crate) struct DissolveCache {
    grids: Vec<NoiseGrid>,
    next: usize,
}

impl DissolveCache {
    pub(crate) fn prepare(&mut self, seed: f32, width: f32, height: f32) -> Option<usize> {
        if !seed.is_finite()
            || ![width, height]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v <= MAX_SAMPLES as f32 && v.fract() == 0.0)
        {
            return None;
        }
        let count = (width as usize).checked_mul(height as usize)?;
        if count > MAX_SAMPLES {
            return None;
        }
        let key = [seed.to_bits(), width.to_bits(), height.to_bits()];
        if let Some(index) = self.grids.iter().position(|grid| grid.key == key) {
            return Some(index);
        }
        // The seed and texel coordinates do not change during a reveal. Keep
        // its expensive noise field, not the animated threshold or burn colour.
        let index = if self.grids.len() < MAX_GRIDS {
            self.grids.push(NoiseGrid {
                key,
                width: 0,
                height: 0,
                samples: Vec::new(),
            });
            self.grids.len() - 1
        } else {
            let index = self.next;
            self.next = (self.next + 1) % MAX_GRIDS;
            index
        };
        let grid = &mut self.grids[index];
        grid.key = key;
        grid.width = width as usize;
        grid.height = height as usize;
        if grid.samples.capacity() < count {
            grid.samples.reserve_exact(count - grid.samples.len());
        }
        grid.samples.resize(count, f32::NAN);
        grid.samples.fill(f32::NAN);
        Some(index)
    }

    #[inline(always)]
    pub(crate) fn grid(&mut self, index: usize) -> &mut NoiseGrid {
        &mut self.grids[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_noise_is_exact_across_reveal_frames_and_texel_boundaries() {
        let mut cache = DissolveCache::default();
        for (w, h) in [(71.0, 95.0), (95.0, 71.0), (17.0, 31.0), (64.0, 128.0)] {
            for seed in [-31.25, 0.0, 78.125, 9125.5] {
                let index = cache.prepare(seed, w, h).unwrap();
                for d in [0.01_f32, 0.2, 0.5, 0.8, 1.0] {
                    let adjusted = (d * d * (3.0 - 2.0 * d)) * 1.02 - 0.01;
                    let field = DissolveField::new(seed, d, adjusted, w, h);
                    for y in -1..=129 {
                        for x in -1..=96 {
                            let ux = x as f32 / w + (y % 3) as f32 * 0.00001;
                            let uy = y as f32 / h - (x % 3) as f32 * 0.00001;
                            assert_eq!(
                                field.sample(ux, uy).to_bits(),
                                field.sample_cached(ux, uy, cache.grid(index)).to_bits()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn noise_cache_reuses_only_matching_fields_and_has_a_fixed_budget() {
        let mut cache = DissolveCache::default();
        for seed in 0..30 {
            let index = cache.prepare(seed as f32, 71.0, 95.0).unwrap();
            assert!(cache.grid(index).samples.iter().all(|v| v.is_nan()));
            cache.grid(index).samples[0] = 0.25;
            assert_eq!(cache.prepare(seed as f32, 71.0, 95.0), Some(index));
            assert_eq!(cache.grid(index).samples[0], 0.25);
        }
        let index = cache.prepare(29.0, 95.0, 71.0).unwrap();
        assert!(cache.grid(index).samples[0].is_nan());
        assert_eq!(cache.grids.len(), MAX_GRIDS);
        assert!(
            cache
                .grids
                .iter()
                .map(|g| g.samples.capacity())
                .sum::<usize>()
                <= MAX_GRIDS * MAX_SAMPLES
        );
        for dims in [
            (8193.0, 1.0),
            (142.0, 190.0),
            (0.0, 0.0),
            (1.5, 2.0),
            (f32::NAN, 2.0),
            (f32::INFINITY, 1.0),
        ] {
            assert!(cache.prepare(0.0, dims.0, dims.1).is_none());
        }
        assert!(cache.prepare(f32::NAN, 71.0, 95.0).is_none());
    }
}
