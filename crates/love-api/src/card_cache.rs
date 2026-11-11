use crate::state::ImageData;
use sprite_to_text::pixel_buffer::DissolveParams;
use sprite_to_text::prepared_card::PreparedCard;
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Weak};

const LIMIT: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;

struct Entry {
    source: Weak<ImageData>,
    region: [u32; 4],
    effect: u8,
    invert: bool,
    /// Quantized shader inputs for edition effects; zero for fixed effects.
    inputs: [u32; 3],
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
    static EDITIONS: LazyLock<bool> =
        LazyLock::new(|| std::env::var("BALATRO_EDITION_CACHE").as_deref() != Ok("0"));
    *ENABLED
        && (matches!(params.shader_effect, 1 | 6)
            || matches!(params.shader_effect, 3 | 4 | 5) && *EDITIONS)
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
                && entry.inputs == [0; 3]
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
        while self.bytes + needed > LIMIT || self.entries.len() >= MAX_ENTRIES {
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
            inputs: [0; 3],
            card: Arc::clone(&card),
        });
        Some(card)
    }

    /// Foil, holo and polychrome shaded once per source region for shader
    /// inputs quantized to a fifth of a second, then reused as sprites.
    /// shortcut: the shimmer updates at ~5 Hz instead of every frame; set
    /// BALATRO_EDITION_CACHE=0 for per-frame shading.
    pub(crate) fn get_shaded(
        &mut self,
        source: &Arc<ImageData>,
        region: [f32; 4],
        mut params: DissolveParams,
    ) -> Option<Arc<PreparedCard>> {
        if region
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 16_777_216.0 || v.fract() != 0.0)
        {
            return None;
        }
        let region = region.map(|v| v as u32);
        let quantize = |v: f32, step: f32| (v / step).floor() * step;
        let inputs = &mut params.shader_inputs;
        inputs.clock = quantize(inputs.clock, 0.2);
        inputs.seed = quantize(inputs.seed, 0.2);
        inputs.phase = quantize(inputs.phase, 0.05);
        let key = [inputs.phase.to_bits(), inputs.clock.to_bits(), inputs.seed.to_bits()];
        let effect = params.shader_effect;
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.source.as_ptr() == Arc::as_ptr(source)
                && entry.region == region
                && entry.effect == effect
                && entry.inputs == key
        }) {
            let entry = self.entries.remove(index).unwrap();
            let card = Arc::clone(&entry.card);
            self.entries.push_back(entry);
            return Some(card);
        }
        let [_, _, w, h] = region;
        let needed = (w * h) as usize * 4 + ((w * h) as usize).div_ceil(8);
        while self.bytes + needed > LIMIT || self.entries.len() >= MAX_ENTRIES {
            let index = self
                .entries
                .iter()
                .position(|entry| Arc::strong_count(&entry.card) == 1)?;
            self.bytes -= self.entries.remove(index)?.card.bytes();
        }
        let card = Arc::new(PreparedCard::shaded(&source.pixels, source.width, region, params)?);
        self.bytes += card.bytes();
        self.entries.push_back(Entry {
            source: Arc::downgrade(source),
            region,
            effect,
            invert: false,
            inputs: key,
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
        let mut held = Vec::new();
        for n in 0..64 {
            match cache.get(&source, [n as f32, 0.0, 256.0, 256.0], 1, false) {
                Some(card) => held.push(card),
                None => break,
            }
        }
        assert!(!held.is_empty() && cache.bytes <= LIMIT, "held draws were evicted");
        assert!(cache
            .get(&source, [100.0, 0.0, 256.0, 256.0], 1, false)
            .is_none());
        held.clear();
        assert!(cache
            .get(&source, [100.0, 0.0, 256.0, 256.0], 1, false)
            .is_some());
        for n in 0..200 {
            cache.get(&source, [n as f32, 0.0, 1.0, 1.0], 1, false);
        }
        assert!(cache.entries.len() <= MAX_ENTRIES && cache.bytes <= LIMIT);
    }

    #[test]
    fn edition_shading_is_reused_within_a_time_step() {
        let source = image();
        let mut cache = CardCache::default();
        let params = |clock: f32| DissolveParams {
            shader_effect: 4,
            shader_inputs: sprite_to_text::pixel_buffer::CardShaderInputs {
                phase: 1.234,
                clock,
                seed: clock,
            },
            ..DissolveParams::NONE
        };
        let region = [0.0, 0.0, 71.0, 95.0];
        let a = cache.get_shaded(&source, region, params(10.02)).unwrap();
        let b = cache.get_shaded(&source, region, params(10.15)).unwrap();
        let c = cache.get_shaded(&source, region, params(10.25)).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "same fifth of a second was reshaded");
        assert!(!Arc::ptr_eq(&a, &c), "next fifth of a second reused stale shading");
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
                shader_effect: 2,
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
