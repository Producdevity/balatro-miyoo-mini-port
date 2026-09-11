use crate::state::{ImageData, Transform};
use sprite_to_text::pixel_buffer::{DissolveParams, PixelBuffer};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Weak};

pub(crate) fn enabled() -> bool {
    static ENABLED: LazyLock<bool> =
        LazyLock::new(|| std::env::var("BALATRO_CARD_OCCLUSION").as_deref() == Ok("1"));
    *ENABLED
}

fn layer_elision_enabled() -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| {
        cfg!(test) || std::env::var("BALATRO_LAYER_ELISION").as_deref() == Ok("1")
    });
    *ENABLED
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect(pub [i32; 4]);

impl Rect {
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        let [a, b, c, d] = self.0;
        let [e, f, g, h] = other.0;
        let result = Self([a.max(e), b.max(f), c.min(g), d.min(h)]);
        (result.0[0] < result.0[2] && result.0[1] < result.0[3]).then_some(result)
    }

    pub(crate) fn area(self) -> i64 {
        i64::from(self.0[2] - self.0[0]) * i64::from(self.0[3] - self.0[1])
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Coverage {
    pub bounds: Option<Rect>,
    pub opaque: Option<Rect>,
    footprint: Option<Footprint>,
    #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
    pub layer: Option<crate::card_layers::Layer>,
}

impl Coverage {
    pub(crate) fn covered_by(self, later: Self) -> bool {
        self.footprint.is_some() && self.footprint == later.footprint
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Footprint {
    // Both queued draws own an Arc<ImageData>, so their identities cannot be
    // recycled while this comparison is possible.
    image: usize,
    region: [u32; 4],
    transform: [u32; 6],
    clip: Rect,
}

#[derive(Clone, Copy)]
struct AlphaShape {
    opaque: Option<[u32; 4]>,
    binary: bool,
    #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
    pair_binary: bool,
}

struct Entry {
    image: Weak<ImageData>,
    region: [u32; 4],
    shape: AlphaShape,
}

pub(crate) struct OpacityCache {
    entries: VecDeque<Entry>,
    limit: usize,
    #[cfg(any(test, feature = "native-sampler"))]
    hits: u64,
    #[cfg(any(test, feature = "native-sampler"))]
    misses: u64,
}

impl Default for OpacityCache {
    fn default() -> Self {
        Self::with_limit(
            std::env::var("BALATRO_OPACITY_CACHE_ENTRIES")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value| (64..=512).contains(value))
                .unwrap_or(256),
        )
    }
}

impl OpacityCache {
    fn with_limit(limit: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(limit),
            limit,
            #[cfg(any(test, feature = "native-sampler"))]
            hits: 0,
            #[cfg(any(test, feature = "native-sampler"))]
            misses: 0,
        }
    }

    fn get(&mut self, image: &Arc<ImageData>, region: [f32; 4]) -> Option<AlphaShape> {
        if region
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 65536.0 || v.fract() != 0.0)
        {
            return None;
        }
        let region = region.map(|v| v as u32);
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.image.as_ptr() == Arc::as_ptr(image) && entry.region == region)
        {
            #[cfg(any(test, feature = "native-sampler"))]
            {
                self.hits += 1;
            }
            return Some(entry.shape);
        }
        let [x, y, w, h] = region;
        if w == 0 || h == 0 || w > 256 || h > 256 || x + w > image.width || y + h > image.height {
            return None;
        }
        let shape = alpha_shape(image, region);
        #[cfg(any(test, feature = "native-sampler"))]
        {
            self.misses += 1;
        }
        if self.entries.len() == self.limit {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            image: Arc::downgrade(image),
            region,
            shape,
        });
        Some(shape)
    }

    pub(crate) fn coverage(
        &mut self,
        image: &Arc<ImageData>,
        region: [f32; 4],
        transform: &Transform,
        params: DissolveParams,
        alpha: u8,
        blend: u8,
        linear: bool,
        clip: Rect,
    ) -> Coverage {
        // Axis-aligned draws use a different scale-rounding path. Keep them out
        // until its sampled footprint is represented here too.
        if linear
            || (transform.b == 0.0 && transform.c == 0.0 && transform.a > 0.0 && transform.d > 0.0)
        {
            return Coverage::default();
        }
        let Some(inverse) = transform.inverse() else {
            return Coverage::default();
        };
        let corners = [
            transform.apply(0.0, 0.0),
            transform.apply(region[2], 0.0),
            transform.apply(region[2], region[3]),
            transform.apply(0.0, region[3]),
        ];
        if corners.iter().any(|(x, y)| {
            !x.is_finite() || !y.is_finite() || x.abs() > 16384.0 || y.abs() > 16384.0
        }) {
            return Coverage::default();
        }
        let bounds = Rect([
            corners
                .iter()
                .map(|p| p.0)
                .fold(f32::INFINITY, f32::min)
                .floor() as i32,
            corners
                .iter()
                .map(|p| p.1)
                .fold(f32::INFINITY, f32::min)
                .floor() as i32,
            corners
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil() as i32,
            corners
                .iter()
                .map(|p| p.1)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil() as i32,
        ])
        .intersect(clip);
        let mut result = Coverage {
            bounds,
            opaque: None,
            footprint: None,
            #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
            layer: None,
        };
        if alpha != 255
            || !matches!(blend, 0 | 4)
            || !matches!(params.shader_effect, 0 | 4 | 5 | 6)
            || params.dissolve > 0.01
        {
            return result;
        }
        if let Some(shape) = self.get(image, region) {
            #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
            if shape.pair_binary && params.dissolve == 0.0 && matches!(params.shader_effect, 4 | 5)
            {
                result.layer = Some(crate::card_layers::Layer {
                    image: Arc::as_ptr(image) as usize,
                    region: region.map(f32::to_bits),
                    transform: [
                        transform.a,
                        transform.b,
                        transform.tx,
                        transform.c,
                        transform.d,
                        transform.ty,
                    ]
                    .map(f32::to_bits),
                    effect: params.shader_effect,
                    inputs: [
                        params.shader_inputs.phase,
                        params.shader_inputs.clock,
                        params.shader_inputs.seed,
                        params.sprite_w,
                        params.sprite_h,
                    ]
                    .map(f32::to_bits),
                    clip,
                    blend,
                });
            }
            result.opaque = shape
                .opaque
                .and_then(|source| inscribed_rect(source, transform, &inverse))
                .and_then(|rect| rect.intersect(clip));
            if shape.binary && layer_elision_enabled() {
                result.footprint = Some(Footprint {
                    image: Arc::as_ptr(image) as usize,
                    region: region.map(f32::to_bits),
                    transform: [
                        transform.a,
                        transform.b,
                        transform.tx,
                        transform.c,
                        transform.d,
                        transform.ty,
                    ]
                    .map(f32::to_bits),
                    clip,
                });
            }
        }
        result
    }
}

#[cfg(feature = "native-sampler")]
impl Drop for OpacityCache {
    fn drop(&mut self) {
        eprintln!(
            "[opacity-cache] capacity={} entries={} hits={} scans={}",
            self.limit,
            self.entries.len(),
            self.hits,
            self.misses
        );
    }
}

// Largest all-opaque rectangle in an atlas region. Only small immutable regions
// enter the bounded cache; no decoded texture or frame is retained by it.
fn alpha_shape(image: &ImageData, [sx, sy, w, h]: [u32; 4]) -> AlphaShape {
    let mut heights = [0u32; 257];
    let mut stack = [0usize; 257];
    let mut best = None;
    let mut area = 0;
    let mut binary = true;
    for y in 0..h {
        for x in 0..w {
            let index = (((sy + y) * image.width + sx + x) * 4 + 3) as usize;
            let alpha = image.pixels.get(index);
            binary &= matches!(alpha, Some(0 | 255));
            heights[x as usize] = if alpha == Some(&255) {
                heights[x as usize] + 1
            } else {
                0
            };
        }
        let mut count = 0;
        for x in 0..=w as usize {
            while count > 0 && heights[stack[count - 1]] > heights[x] {
                count -= 1;
                let height = heights[stack[count]];
                let left = if count == 0 { 0 } else { stack[count - 1] + 1 };
                let width = (x - left) as u32;
                if width * height > area {
                    area = width * height;
                    best = Some([left as u32, y + 1 - height, x as u32, y + 1]);
                }
            }
            stack[count] = x;
            count += 1;
        }
    }
    #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
    let pair_binary = binary && {
        // Adding a large atlas origin can round a sample onto the next texel.
        // Pairing must cover that border too, without changing its sampling.
        let alpha = |x: u32, y: u32| {
            x >= image.width
                || y >= image.height
                || matches!(
                    image.pixels.get(((y * image.width + x) * 4 + 3) as usize),
                    Some(0 | 255)
                )
        };
        (sy..=sy + h).all(|y| alpha(sx + w, y)) && (sx..sx + w).all(|x| alpha(x, sy + h))
    };
    AlphaShape {
        opaque: best,
        binary,
        #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
        pair_binary,
    }
}

fn inscribed_rect(source: [u32; 4], transform: &Transform, inverse: &Transform) -> Option<Rect> {
    let [x0, y0, x1, y1] = source.map(|v| v as f32);
    // Inset the sampled region by a texel, then reserve an output pixel at each
    // edge. This also keeps floating-point boundary rounding out of coverage.
    let hw = (x1 - x0) * 0.5 - 1.0;
    let hh = (y1 - y0) * 0.5 - 1.0;
    if hw <= 0.0 || hh <= 0.0 {
        return None;
    }
    let (cx, cy) = transform.apply((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let hx = transform.a.abs() * hw + transform.b.abs() * hh;
    let hy = transform.c.abs() * hw + transform.d.abs() * hh;
    let scale = (hw / (inverse.a.abs() * hx + inverse.b.abs() * hy))
        .min(hh / (inverse.c.abs() * hx + inverse.d.abs() * hy))
        .min(1.0);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let rect = Rect([
        (cx - hx * scale).ceil() as i32 + 1,
        (cy - hy * scale).ceil() as i32 + 1,
        (cx + hx * scale).floor() as i32 - 1,
        (cy + hy * scale).floor() as i32 - 1,
    ]);
    rect.intersect(rect)
}

pub(crate) fn draw_visible(
    buffer: &mut PixelBuffer,
    hidden: Option<Rect>,
    draw: impl Fn(&mut PixelBuffer),
) {
    let Some(hidden) = hidden else {
        draw(buffer);
        return;
    };
    let previous = buffer.scissor;
    let screen = Rect([0, 0, buffer.width as i32, buffer.height as i32]);
    let clip = match previous {
        Some((x, y, w, h)) => {
            let Some(clip) = Rect([x, y, x.saturating_add(w as i32), y.saturating_add(h as i32)])
                .intersect(screen)
            else {
                return;
            };
            clip
        }
        None => screen,
    };
    let Some(Rect([x0, y0, x1, y1])) = clip.intersect(hidden) else {
        draw(buffer);
        return;
    };
    let [l, t, r, b] = clip.0;
    for rect in [
        Rect([l, t, r, y0]),
        Rect([l, y1, r, b]),
        Rect([l, y0, x0, y1]),
        Rect([x1, y0, r, y1]),
    ] {
        if let Some(Rect([x, y, right, bottom])) = clip.intersect(rect) {
            buffer.scissor = Some((x, y, (right - x) as u32, (bottom - y) as u32));
            draw(buffer);
        }
    }
    buffer.scissor = previous;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_coverage_requires_the_same_binary_alpha_footprint() {
        let mut source = ImageData {
            width: 19,
            height: 17,
            pixels: vec![255; 19 * 17 * 4],
            white_alpha_mask: false,
        };
        for n in (0..19 * 17).step_by(3) {
            source.pixels[n * 4 + 3] = 0;
        }
        let image = Arc::new(source);
        let mut transform = Transform::default();
        transform.translate(31.7, 27.3);
        transform.rotate(0.12);
        let region = [0.0, 0.0, 19.0, 17.0];
        let clip = Rect([0, 0, 160, 120]);
        let mut cache = OpacityCache::default();
        let base = cache.coverage(
            &image,
            region,
            &transform,
            DissolveParams::NONE,
            255,
            0,
            false,
            clip,
        );
        for effect in 0..=11 {
            let params = DissolveParams {
                shader_effect: effect,
                ..DissolveParams::NONE
            };
            let later = cache.coverage(&image, region, &transform, params, 255, 0, false, clip);
            assert_eq!(
                base.covered_by(later),
                matches!(effect, 0 | 4 | 5 | 6),
                "effect={effect}"
            );
        }
        let mut shifted = transform.clone();
        shifted.tx = f32::from_bits(shifted.tx.to_bits() + 1);
        assert!(!base.covered_by(cache.coverage(
            &image,
            region,
            &shifted,
            DissolveParams::NONE,
            255,
            0,
            false,
            clip
        )));
        assert!(!base.covered_by(cache.coverage(
            &image,
            region,
            &transform,
            DissolveParams::NONE,
            254,
            0,
            false,
            clip
        )));
        assert!(!base.covered_by(cache.coverage(
            &image,
            region,
            &transform,
            DissolveParams::NONE,
            255,
            0,
            false,
            Rect([1, 0, 160, 120])
        )));
        let mut partial = ImageData {
            width: image.width,
            height: image.height,
            pixels: image.pixels.clone(),
            white_alpha_mask: false,
        };
        partial.pixels[3] = 254;
        let partial = Arc::new(partial);
        let coverage = cache.coverage(
            &partial,
            region,
            &transform,
            DissolveParams::NONE,
            255,
            0,
            false,
            clip,
        );
        assert!(!coverage.covered_by(coverage));
    }

    #[test]
    fn uncertain_draws_do_not_cover_earlier_pixels() {
        let image = Arc::new(ImageData {
            width: 71,
            height: 95,
            pixels: vec![255; 71 * 95 * 4],
            white_alpha_mask: false,
        });
        let mut t = Transform::default();
        t.rotate(0.03);
        t.translate(30.0, 30.0);
        let mut cache = OpacityCache::default();
        let clip = Rect([0, 0, 640, 480]);
        let region = [0.0, 0.0, 71.0, 95.0];
        assert!(cache
            .coverage(
                &image,
                region,
                &t,
                DissolveParams::NONE,
                255,
                0,
                false,
                clip
            )
            .opaque
            .is_some());
        for effect in [1, 2, 3, 7, 8, 9, 10, 11] {
            let params = DissolveParams {
                shader_effect: effect,
                ..DissolveParams::NONE
            };
            assert!(cache
                .coverage(&image, region, &t, params, 255, 0, false, clip)
                .opaque
                .is_none());
        }
        for effect in [4, 5, 6] {
            let params = DissolveParams {
                shader_effect: effect,
                ..DissolveParams::NONE
            };
            assert!(cache
                .coverage(&image, region, &t, params, 255, 0, false, clip)
                .opaque
                .is_some());
        }
        for (alpha, blend, linear, dissolve) in [
            (254, 0, false, 0.0),
            (255, 2, false, 0.0),
            (255, 3, false, 0.0),
            (255, 0, true, 0.0),
            (255, 0, false, 0.5),
        ] {
            let params = DissolveParams {
                dissolve,
                ..DissolveParams::NONE
            };
            assert!(cache
                .coverage(&image, region, &t, params, alpha, blend, linear, clip)
                .opaque
                .is_none());
        }
        assert!(cache
            .coverage(
                &image,
                region,
                &Transform::default(),
                DissolveParams::NONE,
                255,
                0,
                false,
                clip
            )
            .opaque
            .is_none());
    }

    #[cfg(any(feature = "native-sampler", feature = "layer-pairs"))]
    #[test]
    fn paired_layers_check_alpha_at_atlas_rounding_edges() {
        let mut image = ImageData {
            width: 23,
            height: 21,
            pixels: vec![255; 23 * 21 * 4],
            white_alpha_mask: false,
        };
        let region = [3, 2, 17, 13];
        assert!(alpha_shape(&image, region).pair_binary);
        for (x, y) in [(20, 8), (8, 15), (20, 15)] {
            let index = (y * 23 + x) * 4 + 3;
            image.pixels[index] = 128;
            let shape = alpha_shape(&image, region);
            assert!(shape.binary);
            assert!(!shape.pair_binary);
            image.pixels[index] = 255;
        }
        image.pixels[3] = 128;
        assert!(alpha_shape(&image, region).pair_binary);
        assert!(!alpha_shape(&image, [0, 0, 23, 21]).pair_binary);
    }

    #[test]
    fn opacity_cache_is_bounded_and_does_not_retain_images() {
        let image = Arc::new(ImageData {
            width: 1024,
            height: 8,
            pixels: vec![255; 1024 * 8 * 4],
            white_alpha_mask: false,
        });
        let mut cache = OpacityCache::default();
        for x in 0..cache.limit + 36 {
            assert!(cache.get(&image, [x as f32, 0.0, 8.0, 8.0]).is_some());
        }
        assert_eq!(cache.entries.len(), cache.limit);
        assert_eq!(Arc::strong_count(&image), 1);
        assert!(cache.get(&image, [1023.0, 0.0, 8.0, 8.0]).is_none());
        assert!(cache.get(&image, [0.5, 0.0, 8.0, 8.0]).is_none());
        let weak = Arc::downgrade(&image);
        drop(image);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn repeating_deck_regions_do_not_rescan_the_atlas() {
        let image = Arc::new(ImageData {
            width: 128,
            height: 8,
            pixels: vec![255; 128 * 8 * 4],
            white_alpha_mask: false,
        });
        let mut previous = OpacityCache::with_limit(64);
        let mut current = OpacityCache::with_limit(256);
        for _ in 0..3 {
            for x in 0..96 {
                let region = [x as f32, 0.0, 8.0, 8.0];
                let old = previous.get(&image, region).unwrap();
                let new = current.get(&image, region).unwrap();
                assert_eq!(new.opaque, old.opaque);
                assert_eq!(new.binary, old.binary);
            }
        }
        assert_eq!(previous.misses, 288);
        assert_eq!(current.misses, 96);
        assert_eq!(current.hits, 192);
    }

    #[test]
    fn cached_rectangle_contains_only_opaque_texels() {
        let mut image = ImageData {
            width: 19,
            height: 17,
            pixels: vec![255; 19 * 17 * 4],
            white_alpha_mask: false,
        };
        for y in 0..17 {
            for x in 0..19 {
                if x < 3 || y < 2 || (x > 13 && y > 8) {
                    image.pixels[(y * 19 + x) * 4 + 3] = 200;
                }
            }
        }
        let [x0, y0, x1, y1] = alpha_shape(&image, [0, 0, 19, 17]).opaque.unwrap();
        for y in y0..y1 {
            for x in x0..x1 {
                assert_eq!(image.pixels[((y * 19 + x) * 4 + 3) as usize], 255);
            }
        }
        assert_eq!((x1 - x0) * (y1 - y0), 165);
    }

    #[test]
    fn transformed_coverage_stays_inside_source() {
        for step in -100..100 {
            let mut t = Transform::default();
            t.translate(37.21, 45.67);
            t.rotate(step as f32 * 0.071);
            t.scale(if step % 2 == 0 { 1.3 } else { -1.7 }, 0.87);
            let inv = t.inverse().unwrap();
            if let Some(Rect([x0, y0, x1, y1])) = inscribed_rect([3, 2, 71, 93], &t, &inv) {
                for y in y0..y1 {
                    for x in x0..x1 {
                        let u = inv.a * (x as f32 + 0.5) + (inv.b * (y as f32 + 0.5) + inv.tx);
                        let v = inv.c * (x as f32 + 0.5) + (inv.d * (y as f32 + 0.5) + inv.ty);
                        assert!((3.0..71.0).contains(&u) && (2.0..93.0).contains(&v));
                    }
                }
            }
        }
    }

    #[test]
    fn clipped_replay_preserves_every_visible_pixel_once() {
        for hidden in [
            Rect([5, 7, 13, 19]),
            Rect([-10, -10, 30, 30]),
            Rect([30, 30, 40, 40]),
        ] {
            let mut expected = PixelBuffer::new(23, 25);
            let mut actual = PixelBuffer::new(23, 25);
            expected.scissor = Some((2, 3, 17, 19));
            actual.scissor = expected.scissor;
            expected.fill_rect(0, 0, 23, 25, [21, 57, 193, 111]);
            draw_visible(&mut actual, Some(hidden), |b| {
                b.fill_rect(0, 0, 23, 25, [21, 57, 193, 111])
            });
            expected.scissor = None;
            actual.scissor = None;
            let [x, y, r, b] = hidden.0;
            for target in [&mut expected, &mut actual] {
                target.fill_rect(x, y, r - x, b - y, [1, 2, 3, 255]);
            }
            assert_eq!(actual.pixels, expected.pixels);
        }
    }
}
