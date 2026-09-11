use crate::shader_colour::rgb_to_hsl;

type Hsl = (f32, f32, f32);

#[derive(Clone, Copy)]
struct Entry {
    key: u32,
    hsl: Hsl,
}

pub(crate) struct ColourCache {
    entries: [Entry; 256],
    #[cfg(feature = "shader-cache-stats")]
    hits: u64,
    #[cfg(feature = "shader-cache-stats")]
    misses: u64,
}

impl ColourCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: [Entry {
                key: u32::MAX,
                hsl: (0.0, 0.0, 0.0),
            }; 256],
            #[cfg(feature = "shader-cache-stats")]
            hits: 0,
            #[cfg(feature = "shader-cache-stats")]
            misses: 0,
        }
    }

    #[inline(always)]
    fn convert(&mut self, effect: u8, rgb: [u8; 3]) -> Hsl {
        // Played and negative use the same input conversion.
        let kind = if effect == 6 { 1 } else { effect };
        let key = u32::from_be_bytes([kind, rgb[0], rgb[1], rgb[2]]);
        let slot = (key.wrapping_mul(0x9e3779b1) >> 24) as usize;
        let entry = &mut self.entries[slot];
        if entry.key == key {
            #[cfg(feature = "shader-cache-stats")]
            {
                self.hits += 1;
            }
            return entry.hsl;
        }
        #[cfg(feature = "shader-cache-stats")]
        {
            self.misses += 1;
        }
        let hsl = convert(effect, rgb);
        *entry = Entry { key, hsl };
        hsl
    }

    #[cfg(feature = "shader-cache-stats")]
    pub(crate) fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}

#[inline(always)]
fn convert(effect: u8, [r, g, b]: [u8; 3]) -> Hsl {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    match effect {
        2 => rgb_to_hsl(r * 0.8 + 0.2, g * 0.8, b * 0.8),
        4 => rgb_to_hsl(r * 0.5, g * 0.5, b * 0.5 + 0.5),
        5 => {
            let delta = r.max(g.max(b)) - r.min(g.min(b));
            let saturation = 1.0 - (0.05 * (1.1 - delta)).max(0.0);
            rgb_to_hsl(r * saturation, g * saturation, b)
        }
        _ => rgb_to_hsl(r, g, b),
    }
}

#[inline(always)]
pub(crate) fn card_hsl(cache: Option<&mut ColourCache>, effect: u8, rgb: [u8; 3]) -> Hsl {
    match cache {
        Some(cache) => cache.convert(effect, rgb),
        None => convert(effect, rgb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits((h, s, l): Hsl) -> [u32; 3] {
        [h.to_bits(), s.to_bits(), l.to_bits()]
    }

    #[test]
    fn cache_preserves_all_input_bits_after_hits_and_collisions() {
        let mut cache = ColourCache::new();
        let mut random = 0xb74eda91_u32;
        for _ in 0..100_000 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            let [r, g, b, _] = random.to_le_bytes();
            for effect in [1, 2, 4, 5, 6] {
                let expected = bits(convert(effect, [r, g, b]));
                assert_eq!(bits(cache.convert(effect, [r, g, b])), expected);
                assert_eq!(bits(cache.convert(effect, [r, g, b])), expected);
            }
        }
        assert_eq!(std::mem::size_of::<[Entry; 256]>(), 4096);
    }

    #[test]
    fn black_white_and_grey_are_valid_cache_keys() {
        let mut cache = ColourCache::new();
        for grey in 0..=255 {
            for effect in [1, 2, 4, 5, 6] {
                assert_eq!(
                    bits(cache.convert(effect, [grey; 3])),
                    bits(convert(effect, [grey; 3]))
                );
            }
        }
    }
}
