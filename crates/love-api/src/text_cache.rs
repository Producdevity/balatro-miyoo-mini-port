use crate::state::ImageData;
use crate::text_value::ColoredSegment;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

const MAX_ENTRIES: usize = 512;

#[derive(Default)]
pub(crate) struct TextCache {
    images: HashMap<(u64, u32, Text), Weak<ImageData>>,
}

#[derive(Hash, PartialEq, Eq)]
enum Text {
    Plain(String),
    Colored(Vec<ColoredSegment>),
}

impl TextCache {
    pub(crate) fn rasterize(
        &mut self,
        font: u64,
        size: f32,
        text: &str,
        render: impl FnOnce() -> ImageData,
    ) -> Arc<ImageData> {
        self.get((font, size.to_bits(), Text::Plain(text.to_owned())), render)
    }

    pub(crate) fn rasterize_colored(
        &mut self,
        font: u64,
        size: f32,
        segments: &[ColoredSegment],
        render: impl FnOnce() -> ImageData,
    ) -> Arc<ImageData> {
        self.get(
            (font, size.to_bits(), Text::Colored(segments.to_vec())),
            render,
        )
    }

    fn get(&mut self, key: (u64, u32, Text), render: impl FnOnce() -> ImageData) -> Arc<ImageData> {
        if let Some(image) = self.images.get(&key).and_then(Weak::upgrade) {
            return image;
        }
        let image = Arc::new(render());
        if self.images.len() >= MAX_ENTRIES {
            self.images.retain(|_, image| image.strong_count() > 0);
        }
        if self.images.len() < MAX_ENTRIES {
            self.images.insert(key, Arc::downgrade(&image));
        }
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> ImageData {
        ImageData {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 128],
            white_alpha_mask: true,
        }
    }

    #[test]
    fn identical_text_shares_pixels_but_does_not_keep_them_alive() {
        let mut cache = TextCache::default();
        let first = cache.rasterize(1, 50.0, "A", image);
        let second = cache.rasterize(1, 50.0, "A", || panic!("duplicate rasterization"));
        assert!(Arc::ptr_eq(&first, &second));
        let different_font = cache.rasterize(2, 50.0, "A", image);
        let different_size = cache.rasterize(1, 51.0, "A", image);
        let different_text = cache.rasterize(1, 50.0, "B", image);
        for other in [&different_font, &different_size, &different_text] {
            assert!(!Arc::ptr_eq(&first, other));
        }
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.rasterize(1, 50.0, "A", image).pixels, image().pixels);
    }

    #[test]
    fn live_keys_are_bounded() {
        let mut cache = TextCache::default();
        let images: Vec<_> = (0..MAX_ENTRIES * 2)
            .map(|n| cache.rasterize(1, 50.0, &n.to_string(), image))
            .collect();
        assert_eq!(cache.images.len(), MAX_ENTRIES);
        drop(images);
        cache.rasterize(1, 50.0, "new", image);
        assert_eq!(cache.images.len(), 1);
    }

    #[test]
    fn colored_text_preserves_segment_boundaries_and_alpha() {
        let mut cache = TextCache::default();
        let segments = vec![([255; 4], "A".into()), ([255, 0, 0, 128], "B".into())];
        let first = cache.rasterize_colored(1, 50.0, &segments, image);
        let same = cache.rasterize_colored(1, 50.0, &segments, || panic!("duplicate raster"));
        assert!(Arc::ptr_eq(&first, &same));
        let plain = cache.rasterize(1, 50.0, "AB", image);
        assert!(!Arc::ptr_eq(&first, &plain));
        let mut changed = segments.clone();
        changed[1].0[3] = 255;
        let opaque = cache.rasterize_colored(1, 50.0, &changed, image);
        assert!(!Arc::ptr_eq(&first, &opaque));
        changed = segments.clone();
        changed[0].1 = "AB".into();
        changed[1].1.clear();
        let shifted = cache.rasterize_colored(1, 50.0, &changed, image);
        assert!(!Arc::ptr_eq(&first, &shifted));
    }

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn game_font_measurements_and_cached_pixels_match_rasterization() {
        use crate::state::{FontData, GameSource};
        use parking_lot::Mutex;
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let source = Arc::new(Mutex::new(GameSource::from_path(path.as_ref()).unwrap()));
        let fd = FontData::new(
            source,
            "resources/fonts/m6x11plus.ttf".into(),
            50.0,
            Arc::new(Mutex::new(crate::state::FontCache::default())),
        );
        let font = fd.font().expect("game font must load");
        for size in [-1.0, 0.0, 12.5, 50.0, 200.0] {
            for ch in ' '..='~' {
                let (metrics, _) = font.rasterize(ch, size);
                assert_eq!(
                    fd.text_width_at(&ch.to_string(), size).to_bits(),
                    metrics.advance_width.to_bits()
                );
            }
        }
        let (width, height, pixels) = fd.rasterize_text_at("High Card lvl.1", 50.0);
        let mut cache = TextCache::default();
        let first = cache.rasterize(1, 50.0, "High Card lvl.1", || ImageData {
            width,
            height,
            pixels: pixels.clone(),
            white_alpha_mask: true,
        });
        let second = cache.rasterize(1, 50.0, "High Card lvl.1", || panic!("duplicate raster"));
        assert_eq!(second.pixels, pixels);
        assert!(Arc::ptr_eq(&first, &second));
        // Each Text owns its handle; releasing one must leave the shared pixels alive.
        let mut handles = HashMap::from([(1, first), (2, second)]);
        handles.remove(&1);
        assert_eq!(handles[&2].pixels, pixels);
    }
}
