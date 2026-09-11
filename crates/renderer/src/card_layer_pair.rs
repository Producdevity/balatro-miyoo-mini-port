use super::{DissolveParams, PixelBuffer};

impl PixelBuffer {
    /// Combine two binary-alpha card layers with identical transforms and state.
    /// The caller's immutable-image cache checks that both regions contain only
    /// alpha 0 and 255. Unsupported sampling and effects return false unchanged.
    pub fn draw_card_layer_pair(
        &mut self,
        source: &[u8],
        source_width: u32,
        region: [f32; 4],
        overlay: &[u8],
        overlay_width: u32,
        overlay_region: [f32; 4],
        bounds: [i32; 4],
        inverse: [f32; 6],
        tint: [u8; 4],
        replace: bool,
        params: DissolveParams,
        hidden: Option<[i32; 4]>,
    ) -> bool {
        #[cfg(all(target_arch = "arm", target_endian = "little", feature = "arm-neon"))]
        {
            use crate::card_effects::ShaderPre;
            use crate::shader_batch::ShaderBatch;
            if self.filter_linear
                || tint[3] != 255
                || params.dissolve != 0.0
                || !matches!(params.shader_effect, 4 | 5)
                || region[2..] != overlay_region[2..]
            {
                return false;
            }
            let (left, top, right, bottom) = self.clip_bounds();
            let bounds = [
                bounds[0].max(left),
                bounds[1].max(top),
                bounds[2].min(right),
                bounds[3].min(bottom),
            ];
            if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
                return true;
            }
            let mut pre = ShaderPre::compute(params.shader_effect, params.shader_inputs);
            pre.texture_size = [params.sprite_w.max(1.0), params.sprite_h.max(1.0)];
            let Some(batch) = ShaderBatch::new(params.shader_effect, &pre) else {
                return false;
            };
            self.prepare_shader_colour_cache(params.shader_effect);
            batch.draw_pair(
                self,
                source,
                source_width,
                region,
                overlay,
                overlay_width,
                overlay_region,
                bounds,
                inverse,
                tint,
                replace,
                hidden,
            )
        }
        #[cfg(not(all(target_arch = "arm", target_endian = "little", feature = "arm-neon")))]
        {
            let _ = (
                source,
                source_width,
                region,
                overlay,
                overlay_width,
                overlay_region,
                bounds,
                inverse,
                tint,
                replace,
                params,
                hidden,
            );
            false
        }
    }
}
