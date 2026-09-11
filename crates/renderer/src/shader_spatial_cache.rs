use crate::card_effects::CardShaderInputs;

const MAX_SAMPLES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    effect: u8,
    inputs: [u32; 3],
    texture_size: [u32; 2],
    size: [usize; 2],
}

#[derive(Default)]
pub(crate) struct SpatialCache {
    key: Option<Key>,
    samples: Vec<[f32; 2]>,
    #[cfg(feature = "shader-cache-stats")]
    stats: SpatialCacheStats,
}

#[cfg(feature = "shader-cache-stats")]
#[derive(Clone, Copy, Debug, Default)]
pub struct SpatialCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub reused_draws: u64,
    pub draws: u64,
}

impl SpatialCache {
    pub(crate) fn prepare(
        &mut self,
        effect: u8,
        inputs: CardShaderInputs,
        texture_size: [f32; 2],
        size: [usize; 2],
    ) -> bool {
        let Some(count) = size[0].checked_mul(size[1]) else {
            return false;
        };
        if !matches!(effect, 3..=5)
            || count == 0
            || count > MAX_SAMPLES
            || ![inputs.phase, inputs.clock, inputs.seed]
                .iter()
                .all(|v| v.is_finite())
            || !texture_size.iter().all(|v| v.is_finite() && *v > 0.0)
        {
            return false;
        }
        let key = Key {
            effect,
            inputs: [
                inputs.phase.to_bits(),
                inputs.clock.to_bits(),
                inputs.seed.to_bits(),
            ],
            texture_size: texture_size.map(f32::to_bits),
            size,
        };
        #[cfg(feature = "shader-cache-stats")]
        {
            self.stats.draws += 1;
            self.stats.reused_draws += u64::from(self.key == Some(key));
        }
        if self.key != Some(key) {
            if self.samples.capacity() < count {
                self.samples.reserve_exact(count - self.samples.len());
            }
            self.samples.resize(count, [f32::NAN; 2]);
            self.samples.fill([f32::NAN; 2]);
            self.key = Some(key);
        }
        true
    }

    #[inline(always)]
    pub(crate) fn sample(&mut self, index: usize, compute: impl FnOnce() -> [f32; 2]) -> [f32; 2] {
        let sample = &mut self.samples[index];
        // Transparent or clipped pixels may only become visible on the second layer.
        if sample[0].is_nan() {
            #[cfg(feature = "shader-cache-stats")]
            {
                self.stats.misses += 1;
            }
            *sample = compute();
        } else {
            #[cfg(feature = "shader-cache-stats")]
            {
                self.stats.hits += 1;
            }
        }
        *sample
    }

    #[cfg(feature = "shader-cache-stats")]
    pub(crate) fn stats(&self) -> SpatialCacheStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_layers_reuse_samples_but_each_input_change_invalidates() {
        let mut cache = SpatialCache::default();
        let inputs = CardShaderInputs {
            phase: 1.0,
            clock: 28.0,
            seed: 17.0,
        };
        let texture = [71.0, 95.0];
        let size = [36, 48];
        let cases = [
            (3, inputs, texture, size),
            (4, inputs, texture, size),
            (
                4,
                CardShaderInputs {
                    phase: 1.001,
                    ..inputs
                },
                texture,
                size,
            ),
            (
                4,
                CardShaderInputs {
                    clock: 28.001,
                    ..inputs
                },
                texture,
                size,
            ),
            (
                4,
                CardShaderInputs {
                    seed: 17.001,
                    ..inputs
                },
                texture,
                size,
            ),
            (4, inputs, [142.0, 95.0], size),
            (4, inputs, [71.0, 190.0], size),
            (4, inputs, texture, [48, 36]),
        ];
        for (index, (effect, inputs, texture, size)) in cases.into_iter().enumerate() {
            assert!(cache.prepare(effect, inputs, texture, size));
            let expected = [index as f32, 0.25];
            assert_eq!(cache.sample(5, || expected), expected);
            assert!(cache.prepare(effect, inputs, texture, size));
            assert_eq!(
                cache.sample(5, || panic!("sample should be reused")),
                expected
            );
            assert_eq!(cache.sample(6, || [1.0, 2.0]), [1.0, 2.0]);
        }
    }

    #[test]
    fn invalid_or_large_draws_do_not_allocate() {
        let mut cache = SpatialCache::default();
        let inputs = CardShaderInputs::default();
        for size in [[0, 1], [8193, 1], [usize::MAX, 2]] {
            assert!(!cache.prepare(4, inputs, [71.0, 95.0], size));
        }
        assert!(!cache.prepare(0, inputs, [71.0, 95.0], [36, 48]));
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!cache.prepare(
                4,
                CardShaderInputs {
                    seed: value,
                    ..inputs
                },
                [71.0, 95.0],
                [36, 48]
            ));
        }
        assert_eq!(cache.samples.capacity(), 0);
        for width in [36, 71, 100, 128] {
            assert!(cache.prepare(4, inputs, [71.0, 95.0], [width, 64]));
            assert!(cache.samples.capacity() <= MAX_SAMPLES);
        }
    }
}
