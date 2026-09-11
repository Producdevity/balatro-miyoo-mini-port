use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Instant;

pub(super) static PROFILE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("BALATRO_NATIVE_PROFILE")
        .map(|value| value == "1")
        .unwrap_or(false)
});
pub(super) static AXIS_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static AXIS_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static ROTATED_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static ROTATED_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static NEAREST_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static NEAREST_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static LINEAR_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static LINEAR_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static BOX_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static BOX_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static EFFECT_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static EFFECT_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static AXIS_DRAW_PIXELS: AtomicU64 = AtomicU64::new(0);
pub(super) static MASK_DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static MASK_DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static MASK_DRAW_PIXELS: AtomicU64 = AtomicU64::new(0);
pub(super) static DRAW_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static DRAW_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SIMPLE_FILLS: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SCANLINE_FILLS: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SCANLINE_AXIS: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SCANLINE_TINY_ROTATION: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SCANLINE_SMALL_ROTATION: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_SCANLINE_LARGE_ROTATION: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_POLYGON_LINES: AtomicU64 = AtomicU64::new(0);
pub(super) static UI_TEXT_DRAWS: AtomicU64 = AtomicU64::new(0);
pub(super) static CLEAR_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static CLEAR_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static RECT_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static RECT_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static LINE_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static LINE_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static ELLIPSE_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static ELLIPSE_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static POLYGON_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static POLYGON_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static STENCIL_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static STENCIL_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static DEFERRED_CALLS: AtomicU64 = AtomicU64::new(0);
pub(super) static DEFERRED_NS: AtomicU64 = AtomicU64::new(0);

pub(super) struct ProfileTimer {
    started: Option<Instant>,
    elapsed_ns: &'static AtomicU64,
}

impl ProfileTimer {
    #[inline]
    pub(super) fn start(calls: &'static AtomicU64, elapsed_ns: &'static AtomicU64) -> Self {
        if *PROFILE_ENABLED {
            calls.fetch_add(1, Ordering::Relaxed);
            Self {
                started: Some(Instant::now()),
                elapsed_ns,
            }
        } else {
            Self {
                started: None,
                elapsed_ns,
            }
        }
    }
}

impl Drop for ProfileTimer {
    #[inline]
    fn drop(&mut self) {
        if let Some(started) = self.started {
            self.elapsed_ns
                .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }
}

pub struct DrawProfile {
    pub axis_calls: u64,
    pub axis_ns: u64,
    pub rotated_calls: u64,
    pub rotated_ns: u64,
    pub nearest_calls: u64,
    pub nearest_ns: u64,
    pub linear_calls: u64,
    pub linear_ns: u64,
    pub box_calls: u64,
    pub box_ns: u64,
    pub effect_calls: u64,
    pub effect_ns: u64,
    pub axis_pixels: u64,
    pub mask_calls: u64,
    pub mask_ns: u64,
    pub mask_pixels: u64,
    pub draw_calls: u64,
    pub draw_ns: u64,
    pub ui_cache_hits: u64,
    pub ui_cache_misses: u64,
    pub ui_simple_fills: u64,
    pub ui_scanline_fills: u64,
    pub ui_scanline_axis: u64,
    pub ui_scanline_tiny_rotation: u64,
    pub ui_scanline_small_rotation: u64,
    pub ui_scanline_large_rotation: u64,
    pub ui_polygon_lines: u64,
    pub ui_text_draws: u64,
    pub clear_calls: u64,
    pub clear_ns: u64,
    pub rect_calls: u64,
    pub rect_ns: u64,
    pub line_calls: u64,
    pub line_ns: u64,
    pub ellipse_calls: u64,
    pub ellipse_ns: u64,
    pub polygon_calls: u64,
    pub polygon_ns: u64,
    pub stencil_calls: u64,
    pub stencil_ns: u64,
    pub deferred_calls: u64,
    pub deferred_ns: u64,
}

pub fn take_draw_profile() -> Option<DrawProfile> {
    if !*PROFILE_ENABLED {
        return None;
    }
    Some(DrawProfile {
        axis_calls: AXIS_DRAW_CALLS.swap(0, Ordering::Relaxed),
        axis_ns: AXIS_DRAW_NS.swap(0, Ordering::Relaxed),
        rotated_calls: ROTATED_DRAW_CALLS.swap(0, Ordering::Relaxed),
        rotated_ns: ROTATED_DRAW_NS.swap(0, Ordering::Relaxed),
        nearest_calls: NEAREST_DRAW_CALLS.swap(0, Ordering::Relaxed),
        nearest_ns: NEAREST_DRAW_NS.swap(0, Ordering::Relaxed),
        linear_calls: LINEAR_DRAW_CALLS.swap(0, Ordering::Relaxed),
        linear_ns: LINEAR_DRAW_NS.swap(0, Ordering::Relaxed),
        box_calls: BOX_DRAW_CALLS.swap(0, Ordering::Relaxed),
        box_ns: BOX_DRAW_NS.swap(0, Ordering::Relaxed),
        effect_calls: EFFECT_DRAW_CALLS.swap(0, Ordering::Relaxed),
        effect_ns: EFFECT_DRAW_NS.swap(0, Ordering::Relaxed),
        axis_pixels: AXIS_DRAW_PIXELS.swap(0, Ordering::Relaxed),
        mask_calls: MASK_DRAW_CALLS.swap(0, Ordering::Relaxed),
        mask_ns: MASK_DRAW_NS.swap(0, Ordering::Relaxed),
        mask_pixels: MASK_DRAW_PIXELS.swap(0, Ordering::Relaxed),
        draw_calls: DRAW_CALLS.swap(0, Ordering::Relaxed),
        draw_ns: DRAW_NS.swap(0, Ordering::Relaxed),
        ui_cache_hits: UI_CACHE_HITS.swap(0, Ordering::Relaxed),
        ui_cache_misses: UI_CACHE_MISSES.swap(0, Ordering::Relaxed),
        ui_simple_fills: UI_SIMPLE_FILLS.swap(0, Ordering::Relaxed),
        ui_scanline_fills: UI_SCANLINE_FILLS.swap(0, Ordering::Relaxed),
        ui_scanline_axis: UI_SCANLINE_AXIS.swap(0, Ordering::Relaxed),
        ui_scanline_tiny_rotation: UI_SCANLINE_TINY_ROTATION.swap(0, Ordering::Relaxed),
        ui_scanline_small_rotation: UI_SCANLINE_SMALL_ROTATION.swap(0, Ordering::Relaxed),
        ui_scanline_large_rotation: UI_SCANLINE_LARGE_ROTATION.swap(0, Ordering::Relaxed),
        ui_polygon_lines: UI_POLYGON_LINES.swap(0, Ordering::Relaxed),
        ui_text_draws: UI_TEXT_DRAWS.swap(0, Ordering::Relaxed),
        clear_calls: CLEAR_CALLS.swap(0, Ordering::Relaxed),
        clear_ns: CLEAR_NS.swap(0, Ordering::Relaxed),
        rect_calls: RECT_CALLS.swap(0, Ordering::Relaxed),
        rect_ns: RECT_NS.swap(0, Ordering::Relaxed),
        line_calls: LINE_CALLS.swap(0, Ordering::Relaxed),
        line_ns: LINE_NS.swap(0, Ordering::Relaxed),
        ellipse_calls: ELLIPSE_CALLS.swap(0, Ordering::Relaxed),
        ellipse_ns: ELLIPSE_NS.swap(0, Ordering::Relaxed),
        polygon_calls: POLYGON_CALLS.swap(0, Ordering::Relaxed),
        polygon_ns: POLYGON_NS.swap(0, Ordering::Relaxed),
        stencil_calls: STENCIL_CALLS.swap(0, Ordering::Relaxed),
        stencil_ns: STENCIL_NS.swap(0, Ordering::Relaxed),
        deferred_calls: DEFERRED_CALLS.swap(0, Ordering::Relaxed),
        deferred_ns: DEFERRED_NS.swap(0, Ordering::Relaxed),
    })
}
