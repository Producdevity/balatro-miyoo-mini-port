// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

impl PixelBuffer {
    /// Draw a sub-region of an RGBA source image onto this buffer.
    /// When `replace` is true, pixels are written directly without alpha blending.
    /// When `dissolve` > 0, pixels are randomly discarded based on per-pixel noise
    /// to emulate the dissolve shader effect.
    pub fn draw_image_region(
        &mut self,
        src_pixels: &[u8],
        src_w: u32,
        src_h: u32,
        src_x: f32,
        src_y: f32,
        src_rw: f32,
        src_rh: f32,
        dst_x: f32,
        dst_y: f32,
        sx: f32,
        sy: f32,
        tint: [u8; 4],
        replace: bool,
        source_white_mask: bool,
        dp: DissolveParams,
    ) {
        let abs_sx = sx.abs();
        let abs_sy = sy.abs();
        let dst_w = (src_rw * abs_sx).ceil() as i32;
        let dst_h = (src_rh * abs_sy).ceil() as i32;
        if dst_w <= 0 || dst_h <= 0 {
            return;
        }
        // When scale is negative, the drawing origin shifts
        let dx0 = if sx < 0.0 {
            (dst_x - dst_w as f32) as i32
        } else {
            dst_x as i32
        };
        let dy0 = if sy < 0.0 {
            (dst_y - dst_h as f32) as i32
        } else {
            dst_y as i32
        };

        // Pre-compute clip bounds (buffer intersected with scissor)
        let (clip_x0, clip_y0, clip_x1, clip_y1) = self.clip_bounds();
        let x_start = (clip_x0 - dx0).max(0);
        let y_start = (clip_y0 - dy0).max(0);
        let x_end = dst_w.min(clip_x1 - dx0);
        let y_end = dst_h.min(clip_y1 - dy0);
        if x_start >= x_end || y_start >= y_end {
            return;
        }

        // An unscaled canvas can be copied without another sampling pass.
        let white_tint = tint == [255, 255, 255, 255];
        let plain_draw = dp.dissolve <= 0.01 && dp.shader_effect == 0;
        let identity_scale = (sx - 1.0).abs() < f32::EPSILON && (sy - 1.0).abs() < f32::EPSILON;
        let integral_region = src_x >= 0.0
            && src_y >= 0.0
            && src_x.fract() == 0.0
            && src_y.fract() == 0.0
            && src_rw.fract() == 0.0
            && src_rh.fract() == 0.0;
        if white_tint
            && plain_draw
            && identity_scale
            && integral_region
            && !self.stencil_write_mode
            && self.stencil_compare == StencilCompare::Disabled
        {
            let src_x0 = src_x as i32 + x_start;
            let src_y0 = src_y as i32 + y_start;
            let copy_w = x_end - x_start;
            let copy_h = y_end - y_start;
            let src_in_bounds = src_x0 >= 0
                && src_y0 >= 0
                && src_x0 + copy_w <= src_w as i32
                && src_y0 + copy_h <= src_h as i32;
            let direct_blend = replace || self.blend == 0 || self.blend == 4;

            if src_in_bounds && direct_blend {
                let src_stride = src_w as usize * 4;
                let dst_stride = self.width as usize * 4;
                let row_bytes = copy_w as usize * 4;
                let source_is_opaque = replace
                    || (0..copy_h).all(|row| {
                        let offset = (src_y0 + row) as usize * src_stride + src_x0 as usize * 4;
                        src_pixels[offset..offset + row_bytes]
                            .chunks_exact(4)
                            .all(|pixel| pixel[3] == 255)
                    });

                if source_is_opaque {
                    for row in 0..copy_h {
                        let src_offset = (src_y0 + row) as usize * src_stride + src_x0 as usize * 4;
                        let dst_offset = (dy0 + y_start + row) as usize * dst_stride
                            + (dx0 + x_start) as usize * 4;
                        self.pixels[dst_offset..dst_offset + row_bytes]
                            .copy_from_slice(&src_pixels[src_offset..src_offset + row_bytes]);
                    }
                    return;
                }
            }
        }

        let inv_sx = 1.0 / abs_sx;
        let inv_sy = 1.0 / abs_sy;
        let flip_x = sx < 0.0;
        let flip_y = sy < 0.0;
        let buf_w = self.width;
        let src_len = src_pixels.len();
        let src_rw_minus1 = src_rw - 1.0;
        let src_rh_minus1 = src_rh - 1.0;
        // GLSL-matching dissolve: smooth-step + noise field
        let dissolve_active = dp.dissolve > 0.01;
        let d_raw = dp.dissolve;
        let adjusted_dissolve = if dissolve_active {
            let d = d_raw;
            (d * d * (3.0 - 2.0 * d)) * 1.02 - 0.01
        } else {
            0.0
        };
        let dissolve = dissolve_active.then(|| {
            DissolveField::new(
                dp.shader_inputs.seed,
                d_raw,
                adjusted_dissolve,
                dp.sprite_w,
                dp.sprite_h,
            )
        });
        let dissolve_grid = dissolve_active
            .then(|| {
                self.dissolve_cache
                    .prepare(dp.shader_inputs.seed, dp.sprite_w, dp.sprite_h)
            })
            .flatten();
        let has_burn = dissolve_active && (dp.burn1[3] > 0 || dp.burn2[3] > 0);
        // GLSL burn width: 0.8*(0.5 - |adjusted_dissolve - 0.5|) for outer, 0.5*(...) for inner
        let half_minus = if has_burn {
            (0.5 - (adjusted_dissolve - 0.5).abs()).max(0.0)
        } else {
            0.0
        };
        let burn_outer = 0.8 * half_minus;
        let burn_inner = 0.5 * half_minus;

        // Filtering mode selection:
        // - Box filter (area average) when downscaling >2x — averages all source texels
        //   that map to each destination pixel for correct minification
        // - Bilinear when filter_linear + scale != 1:1 but ratio <= 2x
        // - Nearest otherwise
        // Threshold 2.0: at our ~360px canvas, Balatro draws sprites at ~3.5x smaller scale,
        // so bilinear at >2x would skip >50% of source pixels causing visible aliasing.
        let non_identity = abs_sx < 0.99 || abs_sx > 1.01 || abs_sy < 0.99 || abs_sy > 1.01;
        let use_box = self.filter_linear
            && non_identity
            && (inv_sx > 2.0 || inv_sy > 2.0)
            && (x_end - x_start > 3 && y_end - y_start > 3);
        let use_bilinear = self.filter_linear && non_identity && !use_box;
        let direct_white_mask = source_white_mask && !use_bilinear;
        let src_w_i = src_w as i32;
        // For box filter: source pixel span per dst pixel, capped at 4 samples per axis
        let box_w = if use_box { inv_sx.ceil() as u32 } else { 0 };
        let box_h = if use_box { inv_sy.ceil() as u32 } else { 0 };
        let box_step_x = if box_w > 4 { box_w / 4 } else { 1 };
        let box_step_y = if box_h > 4 { box_h / 4 } else { 1 };
        let src_rx_end = (src_x as u32 + src_rw as u32).min(src_w);
        let src_ry_end = src_y as u32 + src_rh as u32;

        if plain_draw
            && (white_tint || source_white_mask)
            && !self.filter_linear
            && !self.stencil_write_mode
            && self.stencil_compare == StencilCompare::Disabled
            && (replace || self.blend == 0 || self.blend == 4)
        {
            let mut nearest_columns = std::mem::take(&mut self.nearest_columns);
            nearest_columns.clear();
            nearest_columns.reserve((x_end - x_start) as usize);
            for px in x_start..x_end {
                let local_x = px as f32 * inv_sx;
                let fx = src_x
                    + if flip_x {
                        src_rw_minus1 - local_x
                    } else {
                        local_x
                    };
                nearest_columns.push(fx as usize * 4);
            }
            let dst_x_start = (dx0 + x_start) as usize;
            for py in y_start..y_end {
                let local_y = py as f32 * inv_sy;
                let fy = src_y
                    + if flip_y {
                        src_rh_minus1 - local_y
                    } else {
                        local_y
                    };
                let src_row_base = (fy as u32 * src_w) as usize * 4;
                let dst_row_base = ((dy0 + py) as u32 * buf_w) as usize * 4;

                for (column, src_column) in nearest_columns.iter().copied().enumerate() {
                    let si = src_row_base + src_column;
                    if si + 3 >= src_len {
                        continue;
                    }
                    let alpha = if source_white_mask {
                        (src_pixels[si + 3] as u16 * tint[3] as u16 / 255) as u8
                    } else {
                        src_pixels[si + 3]
                    };
                    if alpha == 0 {
                        continue;
                    }

                    let di = dst_row_base + (dst_x_start + column) * 4;
                    if source_white_mask {
                        if replace {
                            self.pixels[di..di + 4]
                                .copy_from_slice(&[tint[0], tint[1], tint[2], alpha]);
                        } else if alpha == 255 {
                            self.pixels[di..di + 4]
                                .copy_from_slice(&[tint[0], tint[1], tint[2], 255]);
                        } else {
                            self.blend_at(di, tint[0], tint[1], tint[2], alpha);
                        }
                    } else if replace || alpha == 255 {
                        self.pixels[di..di + 4].copy_from_slice(&src_pixels[si..si + 4]);
                    } else {
                        self.blend_at(
                            di,
                            src_pixels[si],
                            src_pixels[si + 1],
                            src_pixels[si + 2],
                            alpha,
                        );
                    }
                }
            }
            self.nearest_columns = nearest_columns;
            return;
        }

        // Precompute time-only shader values once per sprite (not per pixel)
        let mut shader_pre = if dp.shader_effect >= 1 {
            ShaderPre::compute(dp.shader_effect, dp.shader_inputs)
        } else {
            ShaderPre::compute(0, CardShaderInputs::default())
        };
        let voucher_effect = matches!(dp.shader_effect, 7 | 8);
        shader_pre.texture_size = [dp.sprite_w.max(1.0), dp.sprite_h.max(1.0)];
        self.prepare_shader_colour_cache(dp.shader_effect);
        let mut shader_batch = if !dissolve_active && !self.stencil_write_mode {
            ShaderBatch::new(dp.shader_effect, &shader_pre)
        } else {
            None
        };
        let spatial_cached = self.shader_spatial_cache_enabled
            && self.shader_spatial_cache.prepare(
                dp.shader_effect,
                dp.shader_inputs,
                shader_pre.texture_size,
                [dst_w as usize, dst_h as usize],
            );
        let mut voucher_columns = if voucher_effect {
            let mut columns = std::mem::take(&mut self.voucher_columns);
            columns.clear();
            columns.reserve((x_end - x_start) as usize);
            for px in x_start..x_end {
                columns.push(VoucherColumn::new(px as f32 / dst_w as f32, shader_pre.t28));
            }
            Some(columns)
        } else {
            None
        };

        for py in y_start..y_end {
            let local_y = py as f32 * inv_sy;
            let fy = src_y
                + if flip_y {
                    src_rh_minus1 - local_y
                } else {
                    local_y
                };
            let src_row = fy as u32;
            let dst_row_base = ((dy0 + py) as u32 * buf_w) as usize * 4;
            let src_row_base = (src_row * src_w) as usize * 4;
            let voucher_row =
                voucher_effect.then(|| VoucherRow::new(py as f32 / dst_h as f32, shader_pre.t28));

            for px in x_start..x_end {
                let local_x = px as f32 * inv_sx;
                let fx = src_x
                    + if flip_x {
                        src_rw_minus1 - local_x
                    } else {
                        local_x
                    };
                let src_col = fx as u32;

                // Sample pixel (box filter, bilinear, or nearest)
                let (sr_raw, sg_raw, sb_raw, sa);
                if use_box {
                    // Box filter: alpha-weighted average to prevent dark fringing at sprite edges.
                    // For opaque pixels (alpha=255) this equals simple averaging.
                    // For sprites with transparency, color is weighted by alpha so that
                    // transparent border pixels don't muddy the sampled color.
                    let bx0 = (fx as u32).min(src_w - 1);
                    let by0 = (fy as u32).min(src_ry_end.saturating_sub(1));
                    if bx0 >= src_rx_end || by0 >= src_ry_end {
                        continue;
                    }
                    let bx1 = (bx0 + box_w).min(src_rx_end);
                    let by1 = (by0 + box_h).min(src_ry_end);
                    let bounds = [bx0, by0, bx1, by1];
                    let step = [box_step_x, box_step_y];
                    let sample = if source_white_mask {
                        average_box::<true>(src_pixels, src_w, bounds, step)
                    } else {
                        average_box::<false>(src_pixels, src_w, bounds, step)
                    };
                    let Some([r, g, b, a]) = sample else {
                        continue;
                    };
                    (sr_raw, sg_raw, sb_raw, sa) = (r, g, b, a);
                } else if use_bilinear {
                    // Bilinear: interpolate 4 surrounding pixels
                    let x0 = fx.floor() as i32;
                    let y0 = fy.floor() as i32;
                    let x1 = x0 + 1;
                    let y1 = y0 + 1;
                    let xf = ((fx - x0 as f32) * 256.0) as u32;
                    let yf = ((fy - y0 as f32) * 256.0) as u32;
                    let ixf = 256 - xf;
                    let iyf = 256 - yf;

                    // Fetch 4 texels (clamped to source bounds — clamp-to-edge)
                    let src_h_max = (src_len / (src_w as usize * 4)).saturating_sub(1) as i32;
                    let cx0 = x0.max(0).min(src_w_i - 1) as usize;
                    let cx1 = x1.max(0).min(src_w_i - 1) as usize;
                    let cy0 = y0.max(0).min(src_h_max) as u32;
                    let cy1 = y1.max(0).min(src_h_max) as u32;
                    let i00 = (cy0 * src_w) as usize * 4 + cx0 * 4;
                    let i10 = (cy0 * src_w) as usize * 4 + cx1 * 4;
                    let i01 = (cy1 * src_w) as usize * 4 + cx0 * 4;
                    let i11 = (cy1 * src_w) as usize * 4 + cx1 * 4;

                    if i11 + 3 >= src_len {
                        continue;
                    } // safety guard

                    let w00 = ixf * iyf;
                    let w10 = xf * iyf;
                    let w01 = ixf * yf;
                    let w11 = xf * yf;

                    sr_raw = ((src_pixels[i00] as u32 * w00
                        + src_pixels[i10] as u32 * w10
                        + src_pixels[i01] as u32 * w01
                        + src_pixels[i11] as u32 * w11)
                        >> 16) as u8;
                    sg_raw = ((src_pixels[i00 + 1] as u32 * w00
                        + src_pixels[i10 + 1] as u32 * w10
                        + src_pixels[i01 + 1] as u32 * w01
                        + src_pixels[i11 + 1] as u32 * w11)
                        >> 16) as u8;
                    sb_raw = ((src_pixels[i00 + 2] as u32 * w00
                        + src_pixels[i10 + 2] as u32 * w10
                        + src_pixels[i01 + 2] as u32 * w01
                        + src_pixels[i11 + 2] as u32 * w11)
                        >> 16) as u8;
                    sa = ((src_pixels[i00 + 3] as u32 * w00
                        + src_pixels[i10 + 3] as u32 * w10
                        + src_pixels[i01 + 3] as u32 * w01
                        + src_pixels[i11 + 3] as u32 * w11)
                        >> 16) as u8;
                } else {
                    let si = src_row_base + src_col as usize * 4;
                    if si + 3 >= src_len {
                        continue;
                    }
                    sr_raw = src_pixels[si];
                    sg_raw = src_pixels[si + 1];
                    sb_raw = src_pixels[si + 2];
                    sa = src_pixels[si + 3];
                }

                if sa == 0 {
                    continue;
                }

                // Dissolve: GLSL-matching noise field + burn edge colors
                let mut burn_override: Option<[u8; 3]> = None;
                if let Some(dissolve) = &dissolve {
                    let ux_d = px as f32 / dst_w as f32;
                    let uy_d = py as f32 / dst_h as f32;
                    let res = match dissolve_grid {
                        Some(index) => {
                            dissolve.sample_cached(ux_d, uy_d, self.dissolve_cache.grid(index))
                        }
                        None => dissolve.sample(ux_d, uy_d),
                    };
                    if res <= adjusted_dissolve {
                        continue;
                    }
                    if has_burn && res < adjusted_dissolve + burn_outer {
                        if res < adjusted_dissolve + burn_inner {
                            burn_override = Some([dp.burn1[0], dp.burn1[1], dp.burn1[2]]);
                        } else if dp.burn2[3] > 0 {
                            burn_override = Some([dp.burn2[0], dp.burn2[1], dp.burn2[2]]);
                        }
                    }
                }

                // Stencil: write or test
                let dst_px = (dx0 + px) as u32;
                let dst_py = (dy0 + py) as u32;
                if self.stencil_write_mode {
                    self.stencil_write(dst_px, dst_py);
                    continue;
                }
                if self.stencil_compare != StencilCompare::Disabled
                    && !self.stencil_test(dst_px, dst_py)
                {
                    continue;
                }

                let (mut sr, mut sg, mut sb, mut fa) = if let Some(bc) = burn_override {
                    (bc[0], bc[1], bc[2], sa)
                } else if white_tint {
                    (sr_raw, sg_raw, sb_raw, sa)
                } else if direct_white_mask {
                    (
                        tint[0],
                        tint[1],
                        tint[2],
                        (sa as u16 * tint[3] as u16 / 255) as u8,
                    )
                } else {
                    (
                        (sr_raw as u16 * tint[0] as u16 / 255) as u8,
                        (sg_raw as u16 * tint[1] as u16 / 255) as u8,
                        (sb_raw as u16 * tint[2] as u16 / 255) as u8,
                        (sa as u16 * tint[3] as u16 / 255) as u8,
                    )
                };

                // GLSL dissolve color tint: blend burn color into entire sprite proportional to dissolve
                if dissolve_active && burn_override.is_none() {
                    let d = d_raw;
                    let mix = 0.6 * d;
                    let inv = 1.0 - mix;
                    if dp.burn2[3] > 0 {
                        sr = (sr as f32 * inv + dp.burn2[0] as f32 * mix) as u8;
                        sg = (sg as f32 * inv + dp.burn2[1] as f32 * mix) as u8;
                        sb = (sb as f32 * inv + dp.burn2[2] as f32 * mix) as u8;
                    } else if dp.burn1[3] > 0 {
                        sr = (sr as f32 * inv + dp.burn1[0] as f32 * mix) as u8;
                        sg = (sg as f32 * inv + dp.burn1[1] as f32 * mix) as u8;
                        sb = (sb as f32 * inv + dp.burn1[2] as f32 * mix) as u8;
                    }
                }

                // Per-pixel shader effects (played/debuff/foil/holo/polychrome/negative/negative_shine/gold_seal)
                // Skip shader on burn-edge pixels: GLSL applies dissolve_mask AFTER the shader,
                // and burn colors replace the shader output — so burn pixels are final.
                if let Some(batch) = shader_batch.as_mut() {
                    batch.push(
                        [sr, sg, sb, fa],
                        [px as f32 / dst_w as f32, py as f32 / dst_h as f32],
                        dst_row_base + dst_px as usize * 4,
                        self.shader_colour_cache.as_deref_mut(),
                    );
                    if batch.full() {
                        self.flush_shader_batch(batch, replace);
                    }
                    continue;
                }
                if dp.shader_effect >= 1 && burn_override.is_none() {
                    if let (Some(columns), Some(row)) = (voucher_columns.as_ref(), voucher_row) {
                        apply_voucher_booster_axis(
                            dp.shader_effect,
                            &shader_pre,
                            columns[(px - x_start) as usize],
                            row,
                            &mut sr,
                            &mut sg,
                            &mut sb,
                            &mut fa,
                        );
                    } else {
                        let ux = px as f32 / dst_w as f32;
                        let uy = py as f32 / dst_h as f32;
                        let sample = spatial_cached.then(|| {
                            self.shader_spatial_cache
                                .sample(py as usize * dst_w as usize + px as usize, || {
                                    spatial(dp.shader_effect, &shader_pre, ux, uy)
                                })
                        });
                        apply_card_shader(
                            dp.shader_effect,
                            &shader_pre,
                            self.shader_colour_cache.as_deref_mut(),
                            sample,
                            ux,
                            uy,
                            dst_px as f32,
                            dst_py as f32,
                            &mut sr,
                            &mut sg,
                            &mut sb,
                            &mut fa,
                        );
                    }
                }

                let di = dst_row_base + dst_px as usize * 4;
                if replace {
                    self.pixels[di] = sr;
                    self.pixels[di + 1] = sg;
                    self.pixels[di + 2] = sb;
                    self.pixels[di + 3] = fa;
                } else if (self.blend == 0 || self.blend == 4) && fa == 255 {
                    // Alpha/premultiplied + fully opaque: direct write (fast path)
                    self.pixels[di] = sr;
                    self.pixels[di + 1] = sg;
                    self.pixels[di + 2] = sb;
                    self.pixels[di + 3] = 255;
                } else if fa > 0 {
                    self.blend_at(di, sr, sg, sb, fa);
                }
            }
        }
        if let Some(columns) = voucher_columns.take() {
            self.voucher_columns = columns;
        }
        if let Some(batch) = shader_batch.as_mut() {
            self.flush_shader_batch(batch, replace);
        }
    }

    /// Draw a sub-region of an RGBA source image using an arbitrary affine transform.
    pub fn draw_image_region_transformed(
        &mut self,
        src_pixels: &[u8],
        src_w: u32,
        src_x: f32,
        src_y: f32,
        src_rw: f32,
        src_rh: f32,
        dst_bounds: (i32, i32, i32, i32),
        inv: [f32; 6],
        tint: [u8; 4],
        replace: bool,
        source_white_mask: bool,
        dp: DissolveParams,
    ) {
        let (min_x, min_y, max_x, max_y) = dst_bounds;
        // Clip to buffer and scissor bounds
        let (clip_x0, clip_y0, clip_x1, clip_y1) = self.clip_bounds();
        let x_start = min_x.max(clip_x0);
        let x_end = max_x.min(clip_x1);
        let y_start = min_y.max(clip_y0);
        let y_end = max_y.min(clip_y1);
        if x_start >= x_end || y_start >= y_end {
            return;
        }

        let [ia, ib, itx, ic, id, ity] = inv;
        if dp.shader_effect == 0
            && dp.dissolve <= 0.01
            && !self.filter_linear
            && !self.stencil_write_mode
            && crate::nearest_raster::plain_enabled()
        {
            self.draw_nearest_affine::<false>(
                src_pixels,
                src_w,
                [src_x, src_y, src_rw, src_rh],
                [x_start, y_start, x_end, y_end],
                inv,
                tint,
                source_white_mask,
                replace,
                None,
            );
            return;
        }
        let white_tint = tint == [255, 255, 255, 255];
        let buf_w = self.width;
        let src_len = src_pixels.len();
        let src_w_i = src_w as i32;
        // Effective scale: how many src pixels per dst pixel along each axis
        let eff_sx = (ia * ia + ic * ic).sqrt();
        let eff_sy = (ib * ib + id * id).sqrt();
        let use_box = self.filter_linear
            && (eff_sx > 2.0 || eff_sy > 2.0)
            && (x_end - x_start > 3 && y_end - y_start > 3);
        let use_bilinear = self.filter_linear && !use_box;
        let direct_white_mask = source_white_mask && !use_bilinear;
        let box_w = if use_box { eff_sx.ceil() as u32 } else { 0 };
        let box_h = if use_box { eff_sy.ceil() as u32 } else { 0 };
        let box_step_x = if box_w > 4 { box_w / 4 } else { 1 };
        let box_step_y = if box_h > 4 { box_h / 4 } else { 1 };
        let src_rx_end = (src_x as u32 + src_rw as u32).min(src_w);
        let src_ry_end = src_y as u32 + src_rh as u32;
        let dissolve_active = dp.dissolve > 0.01;
        let d_raw = dp.dissolve;
        let adjusted_dissolve = if dissolve_active {
            let d = d_raw;
            (d * d * (3.0 - 2.0 * d)) * 1.02 - 0.01
        } else {
            0.0
        };
        let dissolve = dissolve_active.then(|| {
            DissolveField::new(
                dp.shader_inputs.seed,
                d_raw,
                adjusted_dissolve,
                dp.sprite_w,
                dp.sprite_h,
            )
        });
        let dissolve_grid = dissolve_active
            .then(|| {
                self.dissolve_cache
                    .prepare(dp.shader_inputs.seed, dp.sprite_w, dp.sprite_h)
            })
            .flatten();
        let has_burn = dissolve_active && (dp.burn1[3] > 0 || dp.burn2[3] > 0);
        let half_minus = if has_burn {
            (0.5 - (adjusted_dissolve - 0.5).abs()).max(0.0)
        } else {
            0.0
        };
        let burn_outer = 0.8 * half_minus;
        let burn_inner = 0.5 * half_minus;

        // Precompute time-only shader values once per sprite (not per pixel)
        let mut shader_pre = if dp.shader_effect >= 1 {
            ShaderPre::compute(dp.shader_effect, dp.shader_inputs)
        } else {
            ShaderPre::compute(0, CardShaderInputs::default())
        };

        shader_pre.texture_size = [dp.sprite_w.max(1.0), dp.sprite_h.max(1.0)];
        self.prepare_shader_colour_cache(dp.shader_effect);
        let mut shader_batch = if !dissolve_active && !self.stencil_write_mode {
            ShaderBatch::new(dp.shader_effect, &shader_pre)
        } else {
            None
        };
        if !self.filter_linear {
            if let Some(batch) = shader_batch.as_mut() {
                self.draw_nearest_affine::<true>(
                    src_pixels,
                    src_w,
                    [src_x, src_y, src_rw, src_rh],
                    [x_start, y_start, x_end, y_end],
                    inv,
                    tint,
                    source_white_mask,
                    replace,
                    Some(batch),
                );
                return;
            }
        }
        for py in y_start..y_end {
            let pf = py as f32 + 0.5;
            let base_su = ib * pf + itx;
            let base_sv = id * pf + ity;
            let dst_row_base = (py as u32 * buf_w) as usize * 4;

            for px in x_start..x_end {
                let xf = px as f32 + 0.5;
                let su = ia * xf + base_su;
                let sv = ic * xf + base_sv;
                if su < 0.0 || sv < 0.0 || su >= src_rw || sv >= src_rh {
                    continue;
                }

                let abs_su = src_x + su;
                let abs_sv = src_y + sv;
                let src_col = abs_su as u32;
                let src_row = abs_sv as u32;
                if src_col >= src_w {
                    continue;
                }

                // Sample pixel (box filter, bilinear, or nearest)
                let (sr_raw, sg_raw, sb_raw, sa);
                if use_box {
                    // Alpha-weighted box filter (matches draw_image_region)
                    let bx0 = (abs_su as u32).min(src_w - 1);
                    let by0 = (abs_sv as u32).min(src_ry_end.saturating_sub(1));
                    if bx0 >= src_rx_end || by0 >= src_ry_end {
                        continue;
                    }
                    let bx1 = (bx0 + box_w).min(src_rx_end);
                    let by1 = (by0 + box_h).min(src_ry_end);
                    let bounds = [bx0, by0, bx1, by1];
                    let step = [box_step_x, box_step_y];
                    let sample = if source_white_mask {
                        average_box::<true>(src_pixels, src_w, bounds, step)
                    } else {
                        average_box::<false>(src_pixels, src_w, bounds, step)
                    };
                    let Some([r, g, b, a]) = sample else {
                        continue;
                    };
                    (sr_raw, sg_raw, sb_raw, sa) = (r, g, b, a);
                } else if use_bilinear {
                    let x0 = abs_su.floor() as i32;
                    let y0 = abs_sv.floor() as i32;
                    let x1 = x0 + 1;
                    let y1 = y0 + 1;
                    let xf = ((abs_su - x0 as f32) * 256.0) as u32;
                    let yf = ((abs_sv - y0 as f32) * 256.0) as u32;
                    let ixf = 256 - xf;
                    let iyf = 256 - yf;
                    let src_h_max = (src_len / (src_w as usize * 4)).saturating_sub(1) as i32;
                    let cx0 = x0.max(0).min(src_w_i - 1) as usize;
                    let cx1 = x1.max(0).min(src_w_i - 1) as usize;
                    let cy0 = y0.max(0).min(src_h_max) as u32;
                    let cy1 = y1.max(0).min(src_h_max) as u32;
                    let i00 = (cy0 * src_w) as usize * 4 + cx0 * 4;
                    let i10 = (cy0 * src_w) as usize * 4 + cx1 * 4;
                    let i01 = (cy1 * src_w) as usize * 4 + cx0 * 4;
                    let i11 = (cy1 * src_w) as usize * 4 + cx1 * 4;
                    if i11 + 3 >= src_len {
                        continue;
                    } // safety guard
                    let w00 = ixf * iyf;
                    let w10 = xf * iyf;
                    let w01 = ixf * yf;
                    let w11 = xf * yf;
                    sr_raw = ((src_pixels[i00] as u32 * w00
                        + src_pixels[i10] as u32 * w10
                        + src_pixels[i01] as u32 * w01
                        + src_pixels[i11] as u32 * w11)
                        >> 16) as u8;
                    sg_raw = ((src_pixels[i00 + 1] as u32 * w00
                        + src_pixels[i10 + 1] as u32 * w10
                        + src_pixels[i01 + 1] as u32 * w01
                        + src_pixels[i11 + 1] as u32 * w11)
                        >> 16) as u8;
                    sb_raw = ((src_pixels[i00 + 2] as u32 * w00
                        + src_pixels[i10 + 2] as u32 * w10
                        + src_pixels[i01 + 2] as u32 * w01
                        + src_pixels[i11 + 2] as u32 * w11)
                        >> 16) as u8;
                    sa = ((src_pixels[i00 + 3] as u32 * w00
                        + src_pixels[i10 + 3] as u32 * w10
                        + src_pixels[i01 + 3] as u32 * w01
                        + src_pixels[i11 + 3] as u32 * w11)
                        >> 16) as u8;
                } else {
                    let si = ((src_row * src_w + src_col) * 4) as usize;
                    if si + 3 >= src_len {
                        continue;
                    }
                    sr_raw = src_pixels[si];
                    sg_raw = src_pixels[si + 1];
                    sb_raw = src_pixels[si + 2];
                    sa = src_pixels[si + 3];
                }

                if sa == 0 {
                    continue;
                }

                let mut burn_override: Option<[u8; 3]> = None;
                if let Some(dissolve) = &dissolve {
                    let ux_d = su / src_rw;
                    let uy_d = sv / src_rh;
                    let res = match dissolve_grid {
                        Some(index) => {
                            dissolve.sample_cached(ux_d, uy_d, self.dissolve_cache.grid(index))
                        }
                        None => dissolve.sample(ux_d, uy_d),
                    };
                    if res <= adjusted_dissolve {
                        continue;
                    }
                    if has_burn && res < adjusted_dissolve + burn_outer {
                        if res < adjusted_dissolve + burn_inner {
                            burn_override = Some([dp.burn1[0], dp.burn1[1], dp.burn1[2]]);
                        } else if dp.burn2[3] > 0 {
                            burn_override = Some([dp.burn2[0], dp.burn2[1], dp.burn2[2]]);
                        }
                    }
                }

                // Stencil: write or test
                let dpx = px as u32;
                let dpy = py as u32;
                if self.stencil_write_mode {
                    self.stencil_write(dpx, dpy);
                    continue;
                }
                if self.stencil_compare != StencilCompare::Disabled && !self.stencil_test(dpx, dpy)
                {
                    continue;
                }

                let (mut sr, mut sg, mut sb, mut fa) = if let Some(bc) = burn_override {
                    (bc[0], bc[1], bc[2], sa)
                } else if white_tint {
                    (sr_raw, sg_raw, sb_raw, sa)
                } else if direct_white_mask {
                    (
                        tint[0],
                        tint[1],
                        tint[2],
                        (sa as u16 * tint[3] as u16 / 255) as u8,
                    )
                } else {
                    (
                        (sr_raw as u16 * tint[0] as u16 / 255) as u8,
                        (sg_raw as u16 * tint[1] as u16 / 255) as u8,
                        (sb_raw as u16 * tint[2] as u16 / 255) as u8,
                        (sa as u16 * tint[3] as u16 / 255) as u8,
                    )
                };

                // GLSL dissolve color tint: blend burn color into entire sprite proportional to dissolve
                if dissolve_active && burn_override.is_none() {
                    let d = d_raw;
                    let mix = 0.6 * d;
                    let inv = 1.0 - mix;
                    if dp.burn2[3] > 0 {
                        sr = (sr as f32 * inv + dp.burn2[0] as f32 * mix) as u8;
                        sg = (sg as f32 * inv + dp.burn2[1] as f32 * mix) as u8;
                        sb = (sb as f32 * inv + dp.burn2[2] as f32 * mix) as u8;
                    } else if dp.burn1[3] > 0 {
                        sr = (sr as f32 * inv + dp.burn1[0] as f32 * mix) as u8;
                        sg = (sg as f32 * inv + dp.burn1[1] as f32 * mix) as u8;
                        sb = (sb as f32 * inv + dp.burn1[2] as f32 * mix) as u8;
                    }
                }

                if let Some(batch) = shader_batch.as_mut() {
                    batch.push(
                        [sr, sg, sb, fa],
                        [su / src_rw, sv / src_rh],
                        dst_row_base + px as usize * 4,
                        self.shader_colour_cache.as_deref_mut(),
                    );
                    if batch.full() {
                        self.flush_shader_batch(batch, replace);
                    }
                    continue;
                }
                if dp.shader_effect >= 1 && burn_override.is_none() {
                    let ux = su / src_rw;
                    let uy = sv / src_rh;
                    apply_card_shader(
                        dp.shader_effect,
                        &shader_pre,
                        self.shader_colour_cache.as_deref_mut(),
                        None,
                        ux,
                        uy,
                        px as f32,
                        dpy as f32,
                        &mut sr,
                        &mut sg,
                        &mut sb,
                        &mut fa,
                    );
                }

                let di = dst_row_base + px as usize * 4;
                if replace {
                    self.pixels[di] = sr;
                    self.pixels[di + 1] = sg;
                    self.pixels[di + 2] = sb;
                    self.pixels[di + 3] = fa;
                } else if (self.blend == 0 || self.blend == 4) && fa == 255 {
                    self.pixels[di] = sr;
                    self.pixels[di + 1] = sg;
                    self.pixels[di + 2] = sb;
                    self.pixels[di + 3] = 255;
                } else if fa > 0 {
                    self.blend_at(di, sr, sg, sb, fa);
                }
            }
        }
        if let Some(batch) = shader_batch.as_mut() {
            self.flush_shader_batch(batch, replace);
        }
    }
}
