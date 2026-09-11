use super::*;
use crate::occlusion::{Coverage, Rect};
use crate::render_queue::RenderJob;
use sprite_to_text::prepared_card::PreparedCard;

pub(crate) struct ImageDraw {
    image: Arc<ImageData>,
    region: [f32; 4],
    arguments: [f32; 7],
    transform: Transform,
    color: [u8; 4],
    replace: bool,
    params: DissolveParams,
    prepared: Option<Arc<PreparedCard>>,
    state: BufferedDrawState,
}

impl ImageDraw {
    pub(super) fn defer(
        state: &SharedState,
        coverage: Coverage,
        image: &Arc<ImageData>,
        region: [f32; 4],
        arguments: [f32; 7],
        transform: &Transform,
        color: [u8; 4],
        replace: bool,
        params: DissolveParams,
        prepared: Option<Arc<PreparedCard>>,
    ) -> bool {
        if coverage.layer.is_none()
            || !state.can_defer_render()
            || *state.active_canvas.lock() != 0
            || *state.stencil_compare.lock() != StencilCompare::Disabled
        {
            return false;
        }
        let draw = Self {
            image: Arc::clone(image),
            region,
            arguments,
            transform: transform.clone(),
            color,
            replace,
            params,
            prepared,
            state: BufferedDrawState {
                blend: state.blend_code(),
                filter_linear: *state.default_filter_linear.lock(),
                scissor: *state.scissor.lock(),
                stencil_compare: StencilCompare::Disabled,
                stencil_ref: 0,
            },
        };
        let job = RenderJob::image(coverage, draw);
        if let Err(job) = state.submit_render_job(job) {
            state.with_active_buffer(|buffer| job.run(buffer, None));
        }
        true
    }

    pub(crate) fn run(&self, buffer: &mut PixelBuffer, hidden: Option<Rect>) {
        let started = PROFILE_ENABLED.then(Instant::now);
        self.state.apply(buffer);
        let [src_x, src_y, src_w, src_h] = self.region;
        let [x, y, r, sx, sy, ox, oy] = self.arguments;
        crate::occlusion::draw_visible(buffer, hidden, |buffer| {
            draw_region_to_buf(
                buffer,
                &self.image.pixels,
                self.image.width,
                self.image.height,
                src_x,
                src_y,
                src_w,
                src_h,
                x,
                y,
                r,
                sx,
                sy,
                ox,
                oy,
                &self.transform,
                self.color,
                self.replace,
                false,
                self.params,
                self.prepared.as_deref(),
            );
        });
        if let Some(started) = started {
            DEFERRED_CALLS.fetch_add(1, Ordering::Relaxed);
            DEFERRED_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    pub(crate) fn draw_pair(
        &self,
        overlay: &Self,
        layer: crate::card_layers::Layer,
        bounds: Rect,
        buffer: &mut PixelBuffer,
        hidden: Option<Rect>,
    ) -> bool {
        if self.color != overlay.color
            || self.replace != overlay.replace
            || self.state.filter_linear
            || overlay.state.filter_linear
            || self.prepared.is_some()
            || overlay.prepared.is_some()
        {
            return false;
        }
        let [a, b, tx, c, d, ty] = layer.transform.map(f32::from_bits);
        let transform = Transform { a, b, tx, c, d, ty };
        let Some(inverse) = transform.inverse() else {
            return false;
        };
        self.state.apply(buffer);
        buffer.draw_card_layer_pair(
            &self.image.pixels,
            self.image.width,
            self.region,
            &overlay.image.pixels,
            overlay.image.width,
            overlay.region,
            bounds.0,
            [
                inverse.a, inverse.b, inverse.tx, inverse.c, inverse.d, inverse.ty,
            ],
            self.color,
            self.replace,
            self.params,
            hidden.map(|rect| rect.0),
        )
    }
}
