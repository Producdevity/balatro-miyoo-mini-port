// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

impl PixelBuffer {
    /// Approximate background.fs with a sampled swirl field.
    /// Colours are [centre, light, dark]; parameters are [time, spin_time, spin_amount, contrast].
    pub fn fill_procedural_background(
        &mut self,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        colours: &[[f32; 4]; 3],
        bg_params: [f32; 4],
    ) {
        let trig = TrigLookup::new();
        let fast_sin = |x| trig.sin(x);
        let fast_cos = |x| trig.cos(x);
        let bw = self.width as usize;
        let bh = self.height as usize;
        if bw == 0 || bh == 0 {
            return;
        }

        let time = bg_params[0];
        let spin_time = bg_params[1];
        let spin_amount = bg_params[2];
        let contrast = bg_params[3];

        let c1 = colours[0]; // centre color
        let c2 = colours[1]; // light accent
        let c3 = colours[2]; // dark accent

        let screen_len = ((bw * bw + bh * bh) as f32).sqrt();
        let pixel_size = screen_len / 700.0;
        let inv_pixel = 1.0 / pixel_size;

        let half_w = bw as f32 * 0.5;
        let half_h = bh as f32 * 0.5;
        let _mid_x = (bw as f32 / screen_len) * 0.5;
        let _mid_y = (bh as f32 / screen_len) * 0.5;
        let inv_screen_len = 1.0 / screen_len;

        let spin_speed = spin_time * 0.5 * 0.2 + 302.2;
        let paint_speed = time * 2.0;
        let contrast_mod = 0.25 * contrast + 0.5 * spin_amount + 1.2;

        // Sample the field at roughly eight-pixel intervals.
        let gw = (bw / 8).max(30).min(120);
        let gh = (bh / 8).max(20).min(90);
        let row_bytes = bw * 4;

        // Compute paint value at each grid point → (r, g, b)
        let mut grid = vec![(0u8, 0u8, 0u8); (gw + 1) * (gh + 1)];
        for gy in 0..=gh {
            for gx in 0..=gw {
                let px_f = (gx as f32 / gw as f32) * bw as f32;
                let py_f = (gy as f32 / gh as f32) * bh as f32;

                // Pixelation
                let sx = (px_f * inv_pixel).floor() * pixel_size;
                let sy = (py_f * inv_pixel).floor() * pixel_size;
                let mut ux = (sx - half_w) * inv_screen_len - 0.12;
                let mut uy = (sy - half_h) * inv_screen_len;
                let uv_len = (ux * ux + uy * uy).sqrt();

                // Swirl
                let new_angle = uy.atan2(ux) + spin_speed
                    - 0.5 * 20.0 * (spin_amount * uv_len + (1.0 - spin_amount));
                ux = uv_len * fast_cos(new_angle);
                uy = uv_len * fast_sin(new_angle);

                // Paint distortion
                ux *= 30.0;
                uy *= 30.0;
                let mut uv2x = ux + uy;
                let mut uv2y = ux + uy;
                for _ in 0..5 {
                    let mx = if ux > uy { ux } else { uy };
                    let smx = fast_sin(mx);
                    uv2x += smx + ux;
                    uv2y += smx + uy;
                    ux += 0.5 * fast_cos(5.1123314 + 0.353 * uv2y + paint_speed * 0.131121);
                    uy += 0.5 * fast_sin(uv2x - 0.113 * paint_speed);
                    let cxy = fast_cos(ux + uy);
                    let sxy = fast_sin(ux * 0.711 - uy);
                    ux -= cxy - sxy;
                    uy -= cxy - sxy;
                }

                // paint_res: 0..2 range from UV distortion magnitude
                let paint_res = ((ux * ux + uy * uy).sqrt() * 0.035 * contrast_mod)
                    .max(0.0)
                    .min(2.0);
                // c1p peaks when paint_res ≈ 1, c2p peaks when paint_res ≈ 0
                let c1p = (1.0 - contrast_mod * (1.0 - paint_res).abs()).max(0.0);
                let c2p = (1.0 - contrast_mod * paint_res.abs()).max(0.0);
                let c3p = (1.0 - (c1p + c2p).min(1.0)).max(0.0);

                // Final: base tint + weighted color mix
                let base_w = (0.3 / contrast).min(1.0);
                let mix_w = 1.0 - base_w;
                let r = base_w * c1[0] + mix_w * (c1[0] * c1p + c2[0] * c2p + c3[0] * c3p);
                let g = base_w * c1[1] + mix_w * (c1[1] * c1p + c2[1] * c2p + c3[1] * c3p);
                let b = base_w * c1[2] + mix_w * (c1[2] * c1p + c2[2] * c2p + c3[2] * c3p);

                grid[gy * (gw + 1) + gx] = (
                    (r * 255.0).max(0.0).min(255.0) as u8,
                    (g * 255.0).max(0.0).min(255.0) as u8,
                    (b * 255.0).max(0.0).min(255.0) as u8,
                );
            }
        }

        // Fill pixels: bilinear interpolation from grid for smooth transitions
        let inv_gw = gw as f32 / bw as f32;
        let inv_gh = gh as f32 / bh as f32;

        for py in 0..bh {
            let fy = py as f32 * inv_gh;
            let gy0 = (fy as usize).min(gh - 1);
            let gy1 = (gy0 + 1).min(gh);
            let ty = fy - gy0 as f32; // fractional part [0..1)
            let ity = 1.0 - ty;
            let row_base = py * row_bytes;
            let grid_row0 = gy0 * (gw + 1);
            let grid_row1 = gy1 * (gw + 1);

            for px in 0..bw {
                let fx = px as f32 * inv_gw;
                let gx0 = (fx as usize).min(gw - 1);
                let gx1 = (gx0 + 1).min(gw);
                let tx = fx - gx0 as f32;
                let itx = 1.0 - tx;

                // Bilinear: weighted average of 4 surrounding grid points
                let c00 = grid[grid_row0 + gx0];
                let c10 = grid[grid_row0 + gx1];
                let c01 = grid[grid_row1 + gx0];
                let c11 = grid[grid_row1 + gx1];

                let w00 = itx * ity;
                let w10 = tx * ity;
                let w01 = itx * ty;
                let w11 = tx * ty;

                let i = row_base + px * 4;
                self.pixels[i] = (c00.0 as f32 * w00
                    + c10.0 as f32 * w10
                    + c01.0 as f32 * w01
                    + c11.0 as f32 * w11) as u8;
                self.pixels[i + 1] = (c00.1 as f32 * w00
                    + c10.1 as f32 * w10
                    + c01.1 as f32 * w01
                    + c11.1 as f32 * w11) as u8;
                self.pixels[i + 2] = (c00.2 as f32 * w00
                    + c10.2 as f32 * w10
                    + c01.2 as f32 * w01
                    + c11.2 as f32 * w11) as u8;
                self.pixels[i + 3] = 255;
            }
        }
    }

    /// Apply CRT post-processing: bloom (bright glow), contrast adjustment,
    /// vignette (dark edges), and scanlines.
    /// bloom_fac: bloom intensity (from game, typically 0..2), 0 = no bloom
    /// crt_intensity: CRT effect intensity (from game, typically 0..0.16)
    pub fn apply_crt_effect(&mut self, bloom_fac: f32, crt_intensity: f32) {
        let w = self.width as usize;
        let h = self.height as usize;
        if w == 0 || h == 0 {
            return;
        }

        // Normalize CRT intensity (game sends ~0.016 at default setting)
        let crt_norm = (crt_intensity / 0.048).min(1.0).max(0.0);

        // ---- Step 1: Bloom (bright-pass extraction + blur + lerp blend) ----
        // Matches GLSL CRT shader: 7x7 kernel, cutoff 0.6, min-channel threshold
        let bloom_strength =
            bloom_fac.max(0.0).min(2.0) * 0.03 * (crt_norm / (0.16 * 0.3)).max(0.0);
        if bloom_strength > 0.001 {
            // Downsample to small grid for bloom (1/4 resolution for effective blur spread)
            let bw = (w / 4).max(30).min(200);
            let bh = (h / 4).max(20).min(150);
            let cutoff: u16 = 153; // 0.6 * 255 — matches GLSL cutoff

            // Extract bright pass at reduced resolution (u16 to avoid overflow in blur)
            let bloom_size = bw * bh * 3;
            self.crt_bright.resize(bloom_size, 0);
            self.crt_bright[..bloom_size].fill(0);
            let bright = &mut self.crt_bright;
            for by in 0..bh {
                let sy = by * h / bh;
                let src_row = sy * w * 4;
                for bx in 0..bw {
                    let sx = bx * w / bw;
                    let si = src_row + sx * 4;
                    let r = self.pixels[si] as u16;
                    let g = self.pixels[si + 1] as u16;
                    let b = self.pixels[si + 2] as u16;
                    // GLSL: min(r,g,b) thresholded, weighted by distance from center
                    let mn = r.min(g).min(b);
                    let bi = (by * bw + bx) * 3;
                    if mn > cutoff {
                        // Remap: (val - cutoff) / (255 - cutoff) * 255
                        let scale = 255.0 / (255 - cutoff) as f32;
                        bright[bi] = ((r - cutoff) as f32 * scale) as u16;
                        bright[bi + 1] = ((g - cutoff) as f32 * scale) as u16;
                        bright[bi + 2] = ((b - cutoff) as f32 * scale) as u16;
                    }
                }
            }

            // Three passes of 3x3 box blur (separable) = effective ~7x7 kernel
            // Matches GLSL BLOOM_AMT=3 (-3..+3 sample range)
            // At 1/4 resolution this covers ~28 pixels of the source image
            self.crt_temp.resize(bloom_size, 0);
            let temp = &mut self.crt_temp;
            for _pass in 0..3 {
                // Horizontal pass
                for y in 0..bh {
                    let row = y * bw;
                    for x in 0..bw {
                        let x0 = if x > 0 { x - 1 } else { 0 };
                        let x2 = if x + 1 < bw { x + 1 } else { bw - 1 };
                        let i0 = (row + x0) * 3;
                        let i1 = (row + x) * 3;
                        let i2 = (row + x2) * 3;
                        let di = (row + x) * 3;
                        temp[di] = (bright[i0] + bright[i1] + bright[i2]) / 3;
                        temp[di + 1] = (bright[i0 + 1] + bright[i1 + 1] + bright[i2 + 1]) / 3;
                        temp[di + 2] = (bright[i0 + 2] + bright[i1 + 2] + bright[i2 + 2]) / 3;
                    }
                }
                // Vertical pass
                for y in 0..bh {
                    let y0 = if y > 0 { y - 1 } else { 0 };
                    let y2 = if y + 1 < bh { y + 1 } else { bh - 1 };
                    for x in 0..bw {
                        let i0 = (y0 * bw + x) * 3;
                        let i1 = (y * bw + x) * 3;
                        let i2 = (y2 * bw + x) * 3;
                        let di = (y * bw + x) * 3;
                        bright[di] = (temp[i0] + temp[i1] + temp[i2]) / 3;
                        bright[di + 1] = (temp[i0 + 1] + temp[i1 + 1] + temp[i2 + 1]) / 3;
                        bright[di + 2] = (temp[i0 + 2] + temp[i1 + 2] + temp[i2 + 2]) / 3;
                    }
                }
            }

            // Lerp blend with bilinear upsampling: smooth glow, no blocky 4x4 artifacts.
            // Precompute per-column bloom sample positions (8-bit fixed point fractions).
            self.crt_col_bloom.clear();
            self.crt_col_bloom
                .reserve(w.saturating_sub(self.crt_col_bloom.capacity()));
            for x in 0..w {
                let fx = (x as f32 + 0.5) * (bw as f32 / w as f32) - 0.5;
                let bx0 = (fx.max(0.0) as usize).min(bw - 1);
                let bx1 = (bx0 + 1).min(bw - 1);
                let tx = (fx - bx0 as f32).clamp(0.0, 1.0);
                self.crt_col_bloom.push((bx0, bx1, (tx * 256.0) as u32));
            }
            let col_bloom = &self.crt_col_bloom;
            let mix = (bloom_strength * 256.0).min(256.0) as u32;
            let inv_mix = 256 - mix;
            for y in 0..h {
                let fy = (y as f32 + 0.5) * (bh as f32 / h as f32) - 0.5;
                let by0 = (fy.max(0.0) as usize).min(bh - 1);
                let by1 = (by0 + 1).min(bh - 1);
                let ty_fp = ((fy - by0 as f32).clamp(0.0, 1.0) * 256.0) as u32;
                let ity_fp = 256 - ty_fp;
                let row_base = y * w * 4;
                for x in 0..w {
                    let (bx0, bx1, tx_fp) = col_bloom[x];
                    let itx_fp = 256 - tx_fp;
                    let i00 = (by0 * bw + bx0) * 3;
                    let i10 = (by0 * bw + bx1) * 3;
                    let i01 = (by1 * bw + bx0) * 3;
                    let i11 = (by1 * bw + bx1) * 3;
                    let w00 = itx_fp * ity_fp;
                    let w10 = tx_fp * ity_fp;
                    let w01 = itx_fp * ty_fp;
                    let w11 = tx_fp * ty_fp;
                    let br = (bright[i00] as u32 * w00
                        + bright[i10] as u32 * w10
                        + bright[i01] as u32 * w01
                        + bright[i11] as u32 * w11)
                        >> 16;
                    let bg = (bright[i00 + 1] as u32 * w00
                        + bright[i10 + 1] as u32 * w10
                        + bright[i01 + 1] as u32 * w01
                        + bright[i11 + 1] as u32 * w11)
                        >> 16;
                    let bb = (bright[i00 + 2] as u32 * w00
                        + bright[i10 + 2] as u32 * w10
                        + bright[i01 + 2] as u32 * w01
                        + bright[i11 + 2] as u32 * w11)
                        >> 16;
                    let pi = row_base + x * 4;
                    self.pixels[pi] =
                        ((self.pixels[pi] as u32 * inv_mix + br * mix) >> 8).min(255) as u8;
                    self.pixels[pi + 1] =
                        ((self.pixels[pi + 1] as u32 * inv_mix + bg * mix) >> 8).min(255) as u8;
                    self.pixels[pi + 2] =
                        ((self.pixels[pi + 2] as u32 * inv_mix + bb * mix) >> 8).min(255) as u8;
                }
            }
        }

        // ---- Step 2: Contrast adjustment ----
        // Full GLSL CRT transform (lines 79, 102-104):
        //   A = crt_intensity / (0.16*0.3)       -- crt_amout_adjusted
        //   v1 = v * (1 - crt_intensity)
        //   v2 = v1 - (0.55 + 0.014*A*bloom_fac)
        //   v3 = v2 * (1.14 + A*(0.012 - bloom_fac*0.12))
        //   v4 = v3 + 0.5
        // Net: output = input * glsl_scale + glsl_offset (0-1 space)
        //   with bloom=0: scale≈1.037, offset≈-0.142 (slight contrast boost)
        //   with bloom=1: scale≈0.789, offset≈-0.018 (overall darkening)
        if crt_norm > 0.01 {
            let bf = bloom_fac.max(0.0).min(1.0);
            let a_amt = crt_intensity / (0.16 * 0.3); // actual crt_amout_adjusted
            let subtract = 0.55 + 0.014 * a_amt * bf;
            let mult = (1.14 + a_amt * (0.012 - bf * 0.12)).max(0.0);
            let glsl_scale = (1.0 - crt_intensity) * mult;
            let glsl_offset = (0.5 - subtract * mult) * 255.0; // in 0-255 space
                                                               // Blend: t=0 → identity, t=1 → full GLSL; 0.7 factor softens for terminal
            let t = crt_norm.min(1.0) * 0.7;
            let scale = 1.0 + (glsl_scale - 1.0) * t;
            let offset = glsl_offset * t;
            let scale_i = (scale * 256.0) as u32;
            let offset_i = (offset * 256.0) as i32;
            for chunk in self.pixels.chunks_exact_mut(4) {
                for c in &mut chunk[..3] {
                    let v = ((*c as u32 * scale_i) as i32 + offset_i) >> 8;
                    *c = v.max(0).min(255) as u8;
                }
            }
        }

        // Note: GLSL CRT shader uses feather_fac=0.01 (Balatro's fixed value), so the
        // edge mask only activates at pixels outside the screen boundary — no visible
        // vignette at any in-bounds pixel. Scanlines are also skipped because they get
        // averaged away by the terminal downsampler.
    }
}
