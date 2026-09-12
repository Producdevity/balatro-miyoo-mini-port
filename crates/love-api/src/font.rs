// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use crate::game_source::GameSource;
use ab_glyph::{point, Font};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

// Precomputed outlines suit small UI fonts, but expand CJK fonts substantially.
const OUTLINE_CACHE_MAX_BYTES: usize = 256 * 1024;
const LARGE_FONT_IDLE_TIME: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct FontCache {
    large: Option<CachedFont>,
}

struct CachedFont {
    path: String,
    font: Arc<RasterFont>,
    last_used: Instant,
}

impl FontCache {
    fn get(&mut self, path: &str) -> Option<Arc<RasterFont>> {
        let cached = self.large.as_mut().filter(|cached| cached.path == path)?;
        cached.last_used = Instant::now();
        Some(Arc::clone(&cached.font))
    }

    fn insert(&mut self, path: String, font: Arc<RasterFont>) {
        self.large = Some(CachedFont {
            path,
            font,
            last_used: Instant::now(),
        });
    }

    pub fn release_idle(&mut self) {
        if self
            .large
            .as_ref()
            .is_some_and(|cached| cached.last_used.elapsed() >= LARGE_FONT_IDLE_TIME)
        {
            self.large = None;
        }
    }
}

enum LoadedFont {
    Small(Arc<RasterFont>),
    Large,
}

pub(crate) enum RasterFont {
    Cached(fontdue::Font),
    OnDemand(ab_glyph::FontVec),
}

impl RasterFont {
    fn from_large(data: Vec<u8>) -> Result<Self, ab_glyph::InvalidFont> {
        ab_glyph::FontVec::try_from_vec(data).map(Self::OnDemand)
    }

    pub(crate) fn horizontal_line_metrics(&self, size: f32) -> Option<fontdue::LineMetrics> {
        match self {
            Self::Cached(font) => font.horizontal_line_metrics(size),
            Self::OnDemand(font) => {
                let scale = size / font.units_per_em()?;
                let ascent = font.ascent_unscaled() * scale;
                let descent = font.descent_unscaled() * scale;
                let line_gap = font.line_gap_unscaled() * scale;
                Some(fontdue::LineMetrics {
                    ascent,
                    descent,
                    line_gap,
                    new_line_size: ascent - descent + line_gap,
                })
            }
        }
    }

    pub(crate) fn metrics(&self, ch: char, size: f32) -> fontdue::Metrics {
        if size <= 0.0 || !size.is_finite() {
            return fontdue::Metrics::default();
        }
        match self {
            Self::Cached(font) => font.metrics(ch, size),
            Self::OnDemand(font) => fontdue::Metrics {
                advance_width: font.h_advance_unscaled(font.glyph_id(ch)) * size
                    / font.units_per_em().unwrap_or(1.0),
                ..Default::default()
            },
        }
    }

    pub(crate) fn rasterize(&self, ch: char, size: f32) -> (fontdue::Metrics, Vec<u8>) {
        match self {
            Self::Cached(font) => font.rasterize(ch, size),
            Self::OnDemand(font) => {
                let mut metrics = self.metrics(ch, size);
                if size <= 0.0 || !size.is_finite() {
                    return (metrics, Vec::new());
                }
                // ab_glyph scales by line height; the game uses pixels per em.
                let scale = size * font.height_unscaled() / font.units_per_em().unwrap_or(1.0);
                let glyph = font
                    .glyph_id(ch)
                    .with_scale_and_position(scale, point(0.0, 0.0));
                let Some(outline) = font.outline_glyph(glyph) else {
                    return (metrics, Vec::new());
                };
                let bounds = outline.px_bounds();
                metrics.width = bounds.width() as usize;
                metrics.height = bounds.height() as usize;
                metrics.xmin = bounds.min.x as i32;
                metrics.ymin = -bounds.max.y as i32;
                let mut pixels = vec![0; metrics.width * metrics.height];
                outline.draw(|x, y, coverage| {
                    pixels[y as usize * metrics.width + x as usize] =
                        (coverage * 255.0).round() as u8;
                });
                (metrics, pixels)
            }
        }
    }
}

/// A loaded TTF font for text rendering
pub struct FontData {
    source: Arc<Mutex<GameSource>>,
    path: String,
    font: OnceLock<Option<LoadedFont>>,
    cache: Arc<Mutex<FontCache>>,
    advances: Mutex<HashMap<char, f32>>,
    line_metrics: OnceLock<Option<fontdue::LineMetrics>>,
    pub size: f32,
}

impl FontData {
    pub fn new(
        source: Arc<Mutex<GameSource>>,
        path: String,
        size: f32,
        cache: Arc<Mutex<FontCache>>,
    ) -> Self {
        Self {
            source,
            path,
            font: OnceLock::new(),
            cache,
            advances: Mutex::new(HashMap::new()),
            line_metrics: OnceLock::new(),
            size,
        }
    }

    pub(crate) fn font(&self) -> Option<Arc<RasterFont>> {
        let loaded = self.font.get_or_init(|| {
            let mut cache = self.cache.lock();
            cache.large = None;
            let data = self
                .source
                .lock()
                .read_file(&self.path)
                .map_err(|error| {
                    eprintln!("[font] {}: {error}", self.path);
                })
                .ok()?;
            eprintln!("[font] loading {} ({} bytes)", self.path, data.len());
            if data.len() <= OUTLINE_CACHE_MAX_BYTES {
                let font = fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
                    .map_err(|error| eprintln!("[font] {}: {error}", self.path))
                    .ok()?;
                Some(LoadedFont::Small(Arc::new(RasterFont::Cached(font))))
            } else {
                let font = RasterFont::from_large(data)
                    .map_err(|error| eprintln!("[font] {}: {error}", self.path))
                    .ok()?;
                cache.insert(self.path.clone(), Arc::new(font));
                Some(LoadedFont::Large)
            }
        });
        match loaded.as_ref()? {
            LoadedFont::Small(font) => Some(Arc::clone(font)),
            LoadedFont::Large => {
                let mut cache = self.cache.lock();
                if let Some(font) = cache.get(&self.path) {
                    return Some(font);
                }
                // Drop the previous large font before decompressing the next one.
                cache.large = None;
                let data = self
                    .source
                    .lock()
                    .read_file(&self.path)
                    .map_err(|error| eprintln!("[font] {}: {error}", self.path))
                    .ok()?;
                let font = Arc::new(
                    RasterFont::from_large(data)
                        .map_err(|error| eprintln!("[font] {}: {error}", self.path))
                        .ok()?,
                );
                cache.insert(self.path.clone(), Arc::clone(&font));
                Some(font)
            }
        }
    }

    /// Measure the width of a text string at a given size in pixels
    pub fn text_width_at(&self, text: &str, size: f32) -> f32 {
        if matches!(self.font.get(), Some(Some(LoadedFont::Large))) {
            if size <= 0.0 || !size.is_finite() {
                return 0.0;
            }
            let mut advances = self.advances.lock();
            let missing = text.chars().any(|ch| !advances.contains_key(&ch));
            if missing {
                let Some(font) = self.font() else {
                    return 0.0;
                };
                if advances.len() > 8192 {
                    advances.clear();
                }
                for ch in text.chars() {
                    advances
                        .entry(ch)
                        .or_insert_with(|| font.metrics(ch, 1.0).advance_width);
                }
            }
            return text.chars().map(|ch| advances[&ch] * size).sum();
        }
        let Some(font) = self.font() else {
            return text.chars().count() as f32 * (size * 0.6).ceil();
        };
        let mut width = 0.0f32;
        for ch in text.chars() {
            let metrics = if size > 0.0 {
                font.metrics(ch, size)
            } else {
                fontdue::Metrics::default()
            };
            width += metrics.advance_width;
        }
        width
    }

    /// Get the line height at the stored size
    pub fn line_height_at(&self, size: f32) -> f32 {
        if self.font.get().is_none() {
            self.font();
        }
        if let Some(Some(LoadedFont::Small(font))) = self.font.get() {
            return font
                .horizontal_line_metrics(size)
                .map_or(size, |m| m.ascent - m.descent + m.line_gap);
        }
        let metrics = self
            .line_metrics
            .get_or_init(|| self.font()?.horizontal_line_metrics(1.0));
        metrics
            .as_ref()
            .map_or(size, |m| (m.ascent - m.descent + m.line_gap) * size)
    }

    /// Rasterize text into RGBA pixels (white on transparent) at a given size
    /// Returns (width, height, pixels)
    pub fn rasterize_text_at(&self, text: &str, size: f32) -> (u32, u32, Vec<u8>) {
        if text.is_empty() {
            return (0, 0, vec![]);
        }
        let Some(font) = self.font() else {
            return (0, 0, vec![]);
        };

        let metrics = font.horizontal_line_metrics(size);
        let (ascent, height) = match metrics {
            Some(m) => (m.ascent.ceil() as i32, (m.ascent - m.descent).ceil() as u32),
            None => (size as i32, size.ceil() as u32),
        };

        // First pass: measure total width
        let mut total_width = 0.0f32;
        for ch in text.chars() {
            let m = if size > 0.0 {
                font.metrics(ch, size)
            } else {
                fontdue::Metrics::default()
            };
            total_width += m.advance_width;
        }

        let width = total_width.ceil() as u32;
        if width == 0 || height == 0 {
            return (0, 0, vec![]);
        }

        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Second pass: render glyphs
        let mut cursor_x = 0.0f32;
        for ch in text.chars() {
            let (m, bitmap) = font.rasterize(ch, size);
            let bw = m.width;
            let bh = m.height;
            let bmp = &bitmap;

            let glyph_x = cursor_x as i32 + m.xmin;
            let glyph_y = ascent - bh as i32 - m.ymin;

            for gy in 0..bh {
                for gx in 0..bw {
                    let px = glyph_x + gx as i32;
                    let py = glyph_y + gy as i32;
                    if px >= 0 && (px as u32) < width && py >= 0 && (py as u32) < height {
                        let alpha = bmp[gy * bw + gx];
                        if alpha > 0 {
                            let idx = ((py as u32 * width + px as u32) * 4) as usize;
                            pixels[idx] = 255;
                            pixels[idx + 1] = 255;
                            pixels[idx + 2] = 255;
                            pixels[idx + 3] = alpha;
                        }
                    }
                }
            }
            cursor_x += m.advance_width;
        }

        (width, height, pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_fonts_are_released_and_cache_hits_refresh_the_deadline() {
        let mut cache = FontCache::default();
        let font = Arc::new(
            RasterFont::from_large(include_bytes!("../assets/Nunito-Black.ttf").to_vec()).unwrap(),
        );
        let weak = Arc::downgrade(&font);
        cache.insert("font.ttf".to_owned(), font);
        cache.large.as_mut().unwrap().last_used -= LARGE_FONT_IDLE_TIME;
        assert!(cache.get("font.ttf").is_some());
        cache.release_idle();
        assert!(weak.upgrade().is_some());
        cache.large.as_mut().unwrap().last_used -= LARGE_FONT_IDLE_TIME;
        cache.release_idle();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn small_font_keeps_its_line_metrics_before_and_after_loading() {
        let source =
            GameSource::Directory(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"));
        let data = FontData::new(
            Arc::new(Mutex::new(source)),
            "Nunito-Black.ttf".to_owned(),
            40.0,
            Arc::new(Mutex::new(FontCache::default())),
        );
        let reference = fontdue::Font::from_bytes(
            include_bytes!("../assets/Nunito-Black.ttf").as_slice(),
            Default::default(),
        )
        .unwrap();
        for size in [13.7, 40.0, 72.25] {
            let metrics = reference.horizontal_line_metrics(size).unwrap();
            let expected = metrics.ascent - metrics.descent + metrics.line_gap;
            assert_eq!(data.line_height_at(size), expected);
            assert_eq!(data.line_height_at(size), expected);
        }
        assert!(matches!(data.font.get(), Some(Some(LoadedFont::Small(_)))));
    }

    #[test]
    fn on_demand_font_keeps_em_units_and_renders_outlines() {
        let data = include_bytes!("../assets/Nunito-Black.ttf");
        let reference = fontdue::Font::from_bytes(data.as_slice(), Default::default()).unwrap();
        let font = RasterFont::from_large(data.to_vec()).unwrap();
        for size in [12.5, 50.0, 200.0] {
            let actual = font.horizontal_line_metrics(size).unwrap();
            let expected = reference.horizontal_line_metrics(size).unwrap();
            assert!((actual.ascent - expected.ascent).abs() < 0.0001);
            assert!((actual.descent - expected.descent).abs() < 0.0001);
            for ch in ' '..='~' {
                let (metrics, pixels) = font.rasterize(ch, size);
                assert!(
                    (metrics.advance_width - reference.metrics(ch, size).advance_width).abs()
                        < 0.0001
                );
                assert_eq!(pixels.len(), metrics.width * metrics.height);
                if ch != ' ' {
                    assert!(pixels.iter().any(|&v| v != 0), "{ch}");
                }
            }
        }
        for size in [-1.0, 0.0, f32::NAN] {
            assert!(font.rasterize('A', size).1.is_empty());
        }
    }

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn language_fonts_render_and_release_the_previous_file() {
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let source = Arc::new(Mutex::new(GameSource::from_path(path.as_ref()).unwrap()));
        let cache = Arc::new(Mutex::new(FontCache::default()));
        let mut previous = None;
        for (name, text) in [
            ("NotoSansSC-Bold.ttf", "简体中文"),
            ("NotoSansTC-Bold.ttf", "繁體中文"),
            ("NotoSansJP-Bold.ttf", "日本語"),
            ("NotoSansKR-Bold.ttf", "한국어"),
            ("NotoSans-Bold.ttf", "Русский"),
        ] {
            let data = FontData::new(
                Arc::clone(&source),
                format!("resources/fonts/{name}"),
                40.0,
                Arc::clone(&cache),
            );
            let font = data.font().unwrap();
            if let Some(previous) = previous.take() {
                assert!(std::sync::Weak::<RasterFont>::upgrade(&previous).is_none());
            }
            let RasterFont::OnDemand(face) = &*font else {
                panic!("expected on-demand font")
            };
            for ch in text.chars() {
                assert_ne!(face.glyph_id(ch).0, 0, "{name}: {ch}");
            }
            let (width, height, pixels) = data.rasterize_text_at(text, 40.0);
            assert!(width > 0 && height > 0);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
            assert_eq!(data.text_width_at(text, 40.0).ceil() as u32, width);
            assert_eq!(data.text_width_at(text, 40.0).ceil() as u32, width);
            let unloaded = Arc::downgrade(&font);
            drop(font);
            cache.lock().large.as_mut().unwrap().last_used -= LARGE_FONT_IDLE_TIME;
            cache.lock().release_idle();
            assert!(cache.lock().large.is_none());
            assert!(unloaded.upgrade().is_none());
            assert_eq!(data.text_width_at(text, 40.0).ceil() as u32, width);
            assert!(
                cache.lock().large.is_none(),
                "width measurement reloaded the font"
            );
            let reloaded = data.rasterize_text_at(text, 40.0);
            assert_eq!(reloaded, (width, height, pixels));
            previous = Some(Arc::downgrade(&data.font().unwrap()));
        }
    }
}
