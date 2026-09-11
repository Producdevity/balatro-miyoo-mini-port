use crate::state::ImageData;
use sprite_to_text::pixel_buffer::DissolveParams;
use sprite_to_text::prepared_card::PreparedCard;
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Weak};

const LIMIT: usize = 512 * 1024;

struct Entry {
    source: Weak<ImageData>,
    region: [u32; 4],
    effect: u8,
    invert: bool,
    card: Arc<PreparedCard>,
}

#[derive(Default)]
pub(crate) struct CardCache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

pub(crate) fn eligible(params: DissolveParams, color: [u8; 4], linear: bool) -> bool {
    static ENABLED: LazyLock<bool> =
        LazyLock::new(|| std::env::var("BALATRO_PREPARED_CARDS").as_deref() != Ok("0"));
    *ENABLED
        && matches!(params.shader_effect, 1 | 6)
        && params.dissolve <= 0.01
        && color == [255; 4]
        && !linear
}

impl CardCache {
    pub(crate) fn get(
        &mut self,
        source: &Arc<ImageData>,
        region: [f32; 4],
        effect: u8,
        invert: bool,
    ) -> Option<Arc<PreparedCard>> {
        if region
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 16_777_216.0 || v.fract() != 0.0)
        {
            return None;
        }
        let region = region.map(|v| v as u32);
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.source.as_ptr() == Arc::as_ptr(source)
                && entry.region == region
                && entry.effect == effect
                && entry.invert == invert
        }) {
            let entry = self.entries.remove(index).unwrap();
            let card = Arc::clone(&entry.card);
            self.entries.push_back(entry);
            return Some(card);
        }
        let [_, _, w, h] = region;
        if w == 0 || h == 0 || w > 256 || h > 256 {
            return None;
        }
        let texels = ((w + 1) * (h + 1)) as usize;
        let needed = texels * 4 + texels.div_ceil(8);
        while self.bytes + needed > LIMIT || self.entries.len() >= 32 {
            // Queued draws may still own a card. Do not evict it and allocate over the limit.
            let index = self
                .entries
                .iter()
                .position(|entry| Arc::strong_count(&entry.card) == 1)?;
            self.bytes -= self.entries.remove(index)?.card.bytes();
        }
        let card = Arc::new(PreparedCard::new(
            &source.pixels,
            source.width,
            region,
            effect,
            invert,
        )?);
        self.bytes += card.bytes();
        self.entries.push_back(Entry {
            source: Arc::downgrade(source),
            region,
            effect,
            invert,
            card: Arc::clone(&card),
        });
        Some(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Arc<ImageData> {
        Arc::new(ImageData {
            width: 512,
            height: 512,
            pixels: vec![127; 512 * 512 * 4],
            white_alpha_mask: false,
        })
    }

    #[test]
    fn reuses_only_matching_source_region_and_effect() {
        let source = image();
        let mut cache = CardCache::default();
        let a = cache
            .get(&source, [0.0, 0.0, 71.0, 95.0], 1, false)
            .unwrap();
        assert!(Arc::ptr_eq(
            &a,
            &cache
                .get(&source, [0.0, 0.0, 71.0, 95.0], 1, false)
                .unwrap()
        ));
        for (image, region, effect, invert) in [
            (Arc::clone(&source), [71.0, 0.0, 71.0, 95.0], 1, false),
            (Arc::clone(&source), [0.0, 0.0, 71.0, 95.0], 6, false),
            (Arc::clone(&source), [0.0, 0.0, 71.0, 95.0], 6, true),
            (image(), [0.0, 0.0, 71.0, 95.0], 1, false),
        ] {
            assert!(!Arc::ptr_eq(
                &a,
                &cache.get(&image, region, effect, invert).unwrap()
            ));
        }
        assert_eq!(Arc::strong_count(&source), 1);
    }

    #[test]
    fn pending_draws_cannot_push_the_cache_over_budget() {
        let source = image();
        let mut cache = CardCache::default();
        let first = cache
            .get(&source, [0.0, 0.0, 256.0, 256.0], 1, false)
            .unwrap();
        assert!(cache
            .get(&source, [1.0, 0.0, 256.0, 256.0], 1, false)
            .is_none());
        drop(first);
        assert!(cache
            .get(&source, [1.0, 0.0, 256.0, 256.0], 1, false)
            .is_some());
        assert!(cache.bytes <= LIMIT);
        for n in 0..100 {
            cache.get(&source, [n as f32, 0.0, 1.0, 1.0], 1, false);
        }
        assert!(cache.entries.len() <= 32 && cache.bytes <= LIMIT);
    }

    #[test]
    fn unsupported_draws_stay_on_the_original_path() {
        let params = DissolveParams {
            shader_effect: 1,
            ..DissolveParams::NONE
        };
        assert!(eligible(params, [255; 4], false));
        assert!(!eligible(params, [255; 4], true));
        assert!(!eligible(params, [254; 4], false));
        assert!(!eligible(
            DissolveParams {
                dissolve: 0.5,
                ..params
            },
            [255; 4],
            false
        ));
        assert!(!eligible(
            DissolveParams {
                shader_effect: 5,
                ..params
            },
            [255; 4],
            false
        ));
        let mut cache = CardCache::default();
        for region in [
            [0.5, 0.0, 71.0, 95.0],
            [f32::NAN, 0.0, 71.0, 95.0],
            [0.0, 0.0, 512.0, 512.0],
        ] {
            assert!(cache.get(&image(), region, 1, false).is_none());
        }
    }
}
