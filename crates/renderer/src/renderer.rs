// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use std::cell::RefCell;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::pixel_buffer::PixelBuffer;

/// Reusable buffers for Sixel encoding to avoid per-frame allocations.
/// Stored in thread_local, cleared with fill(0)/clear() instead of reallocated.
struct SixelBuffers {
    // quantize_median_cut histogram buffers (~928KB)
    counts: Vec<u32>, // [32768] histogram counts
    r_sums: Vec<u64>, // [32768] red channel sums
    g_sums: Vec<u64>, // [32768] green channel sums
    b_sums: Vec<u64>, // [32768] blue channel sums
    lut: Vec<u8>,     // [32768] 5-bit RGB key → palette index
    indices: Vec<u8>, // [pixel_count] per-pixel palette indices
    // Sixel output buffer (~600KB-3MB)
    output: Vec<u8>,
    // Band encoding buffers (~492KB + 256B)
    color_bits: Vec<u8>, // [n_colors * sw]
    used: Vec<bool>,     // [n_colors]
    active_colors: Vec<usize>,
    // col_widths cache (recomputed only on resize)
    col_widths: Vec<u32>,
    col_widths_key: (usize, usize), // (sw, out_w) — invalidation key
}

thread_local! {
    static SIXEL_BUF: RefCell<SixelBuffers> = RefCell::new(SixelBuffers {
        counts: vec![0u32; 32768],
        r_sums: vec![0u64; 32768],
        g_sums: vec![0u64; 32768],
        b_sums: vec![0u64; 32768],
        lut: vec![0u8; 32768],
        indices: Vec::with_capacity(300_000),
        output: Vec::with_capacity(1_000_000),
        color_bits: Vec::with_capacity(256 * 1024),
        used: Vec::with_capacity(256),
        active_colors: Vec::with_capacity(64),
        col_widths: Vec::new(),
        col_widths_key: (0, 0),
    });
}

/// sRGB → linear lookup table (256 entries, sRGB 0..255 → linear 0..65535).
/// Computed once at startup using the exact sRGB transfer function.
static SRGB_TO_LINEAR: std::sync::LazyLock<[u16; 256]> = std::sync::LazyLock::new(|| {
    let mut lut = [0u16; 256];
    for i in 0..256 {
        let s = i as f64 / 255.0;
        let lin = if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        };
        lut[i] = (lin * 65535.0 + 0.5) as u16;
    }
    lut
});

/// Linear → sRGB lookup table (1024 entries for smooth dark-end precision).
/// Maps linear value index (0..1023) → sRGB 0..255.
static LINEAR_TO_SRGB: std::sync::LazyLock<[u8; 1024]> = std::sync::LazyLock::new(|| {
    let mut lut = [0u8; 1024];
    for i in 0..1024 {
        let lin = i as f64 / 1023.0;
        let s = if lin <= 0.0031308 {
            lin * 12.92
        } else {
            1.055 * lin.powf(1.0 / 2.4) - 0.055
        };
        lut[i] = (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    lut
});

/// Convert a linear-space average (0..65535) back to sRGB (0..255)
/// using the precomputed 1024-entry lookup table.
#[inline(always)]
fn linear_to_srgb(v: u32, l2s: &[u8; 1024]) -> u8 {
    let idx = (v >> 6) as usize;
    l2s[idx.min(1023)]
}

/// Octant character lookup table: maps 8-bit pattern → Unicode character.
/// Bit layout per cell (2 columns × 4 rows):
///   bit0 bit1   (row 0)
///   bit2 bit3   (row 1)
///   bit4 bit5   (row 2)
///   bit6 bit7   (row 3)
/// Derived from Unicode 16.0 block octant characters (U+1CD00–U+1CDE5),
/// scattered block elements, and pre-existing quadrant/half-block characters.
#[rustfmt::skip]
const OCTANT_CHARS: [char; 256] = [
    // 0x00..0x07
    ' ',            '\u{1CEA8}', '\u{1CEAB}', '\u{1FB82}', '\u{1CD00}', '\u{2598}',  '\u{1CD01}', '\u{1CD02}',
    // 0x08..0x0F
    '\u{1CD03}', '\u{1CD04}', '\u{259D}',  '\u{1CD05}', '\u{1CD06}', '\u{1CD07}', '\u{1CD08}', '\u{2580}',
    // 0x10..0x17
    '\u{1CD09}', '\u{1CD0A}', '\u{1CD0B}', '\u{1CD0C}', '\u{1FBE6}', '\u{1CD0D}', '\u{1CD0E}', '\u{1CD0F}',
    // 0x18..0x1F
    '\u{1CD10}', '\u{1CD11}', '\u{1CD12}', '\u{1CD13}', '\u{1CD14}', '\u{1CD15}', '\u{1CD16}', '\u{1CD17}',
    // 0x20..0x27
    '\u{1CD18}', '\u{1CD19}', '\u{1CD1A}', '\u{1CD1B}', '\u{1CD1C}', '\u{1CD1D}', '\u{1CD1E}', '\u{1CD1F}',
    // 0x28..0x2F
    '\u{1FBE7}', '\u{1CD20}', '\u{1CD21}', '\u{1CD22}', '\u{1CD23}', '\u{1CD24}', '\u{1CD25}', '\u{1CD26}',
    // 0x30..0x37
    '\u{1CD27}', '\u{1CD28}', '\u{1CD29}', '\u{1CD2A}', '\u{1CD2B}', '\u{1CD2C}', '\u{1CD2D}', '\u{1CD2E}',
    // 0x38..0x3F
    '\u{1CD2F}', '\u{1CD30}', '\u{1CD31}', '\u{1CD32}', '\u{1CD33}', '\u{1CD34}', '\u{1CD35}', '\u{1FB85}',
    // 0x40..0x47
    '\u{1CEA3}', '\u{1CD36}', '\u{1CD37}', '\u{1CD38}', '\u{1CD39}', '\u{1CD3A}', '\u{1CD3B}', '\u{1CD3C}',
    // 0x48..0x4F
    '\u{1CD3D}', '\u{1CD3E}', '\u{1CD3F}', '\u{1CD40}', '\u{1CD41}', '\u{1CD42}', '\u{1CD43}', '\u{1CD44}',
    // 0x50..0x57
    '\u{2596}',  '\u{1CD45}', '\u{1CD46}', '\u{1CD47}', '\u{1CD48}', '\u{258C}',  '\u{1CD49}', '\u{1CD4A}',
    // 0x58..0x5F
    '\u{1CD4B}', '\u{1CD4C}', '\u{259E}',  '\u{1CD4D}', '\u{1CD4E}', '\u{1CD4F}', '\u{1CD50}', '\u{259B}',
    // 0x60..0x67
    '\u{1CD51}', '\u{1CD52}', '\u{1CD53}', '\u{1CD54}', '\u{1CD55}', '\u{1CD56}', '\u{1CD57}', '\u{1CD58}',
    // 0x68..0x6F
    '\u{1CD59}', '\u{1CD5A}', '\u{1CD5B}', '\u{1CD5C}', '\u{1CD5D}', '\u{1CD5E}', '\u{1CD5F}', '\u{1CD60}',
    // 0x70..0x77
    '\u{1CD61}', '\u{1CD62}', '\u{1CD63}', '\u{1CD64}', '\u{1CD65}', '\u{1CD66}', '\u{1CD67}', '\u{1CD68}',
    // 0x78..0x7F
    '\u{1CD69}', '\u{1CD6A}', '\u{1CD6B}', '\u{1CD6C}', '\u{1CD6D}', '\u{1CD6E}', '\u{1CD6F}', '\u{1CD70}',
    // 0x80..0x87
    '\u{1CEA0}', '\u{1CD71}', '\u{1CD72}', '\u{1CD73}', '\u{1CD74}', '\u{1CD75}', '\u{1CD76}', '\u{1CD77}',
    // 0x88..0x8F
    '\u{1CD78}', '\u{1CD79}', '\u{1CD7A}', '\u{1CD7B}', '\u{1CD7C}', '\u{1CD7D}', '\u{1CD7E}', '\u{1CD7F}',
    // 0x90..0x97
    '\u{1CD80}', '\u{1CD81}', '\u{1CD82}', '\u{1CD83}', '\u{1CD84}', '\u{1CD85}', '\u{1CD86}', '\u{1CD87}',
    // 0x98..0x9F
    '\u{1CD88}', '\u{1CD89}', '\u{1CD8A}', '\u{1CD8B}', '\u{1CD8C}', '\u{1CD8D}', '\u{1CD8E}', '\u{1CD8F}',
    // 0xA0..0xA7
    '\u{2597}',  '\u{1CD90}', '\u{1CD91}', '\u{1CD92}', '\u{1CD93}', '\u{259A}',  '\u{1CD94}', '\u{1CD95}',
    // 0xA8..0xAF
    '\u{1CD96}', '\u{1CD97}', '\u{2590}',  '\u{1CD98}', '\u{1CD99}', '\u{1CD9A}', '\u{1CD9B}', '\u{259C}',
    // 0xB0..0xB7
    '\u{1CD9C}', '\u{1CD9D}', '\u{1CD9E}', '\u{1CD9F}', '\u{1CDA0}', '\u{1CDA1}', '\u{1CDA2}', '\u{1CDA3}',
    // 0xB8..0xBF
    '\u{1CDA4}', '\u{1CDA5}', '\u{1CDA6}', '\u{1CDA7}', '\u{1CDA8}', '\u{1CDA9}', '\u{1CDAA}', '\u{1CDAB}',
    // 0xC0..0xC7
    '\u{2582}',  '\u{1CDAC}', '\u{1CDAD}', '\u{1CDAE}', '\u{1CDAF}', '\u{1CDB0}', '\u{1CDB1}', '\u{1CDB2}',
    // 0xC8..0xCF
    '\u{1CDB3}', '\u{1CDB4}', '\u{1CDB5}', '\u{1CDB6}', '\u{1CDB7}', '\u{1CDB8}', '\u{1CDB9}', '\u{1CDBA}',
    // 0xD0..0xD7
    '\u{1CDBB}', '\u{1CDBC}', '\u{1CDBD}', '\u{1CDBE}', '\u{1CDBF}', '\u{1CDC0}', '\u{1CDC1}', '\u{1CDC2}',
    // 0xD8..0xDF
    '\u{1CDC3}', '\u{1CDC4}', '\u{1CDC5}', '\u{1CDC6}', '\u{1CDC7}', '\u{1CDC8}', '\u{1CDC9}', '\u{1CDCA}',
    // 0xE0..0xE7
    '\u{1CDCB}', '\u{1CDCC}', '\u{1CDCD}', '\u{1CDCE}', '\u{1CDCF}', '\u{1CDD0}', '\u{1CDD1}', '\u{1CDD2}',
    // 0xE8..0xEF
    '\u{1CDD3}', '\u{1CDD4}', '\u{1CDD5}', '\u{1CDD6}', '\u{1CDD7}', '\u{1CDD8}', '\u{1CDD9}', '\u{1CDDA}',
    // 0xF0..0xF7
    '\u{2584}',  '\u{1CDDB}', '\u{1CDDC}', '\u{1CDDD}', '\u{1CDDE}', '\u{2599}',  '\u{1CDDF}', '\u{1CDE0}',
    // 0xF8..0xFF
    '\u{1CDE1}', '\u{1CDE2}', '\u{259F}',  '\u{1CDE3}', '\u{2586}',  '\u{1CDE4}', '\u{1CDE5}', '\u{2588}',
];

/// Widget that renders a PixelBuffer using octant characters (2×4 sub-pixels per cell).
/// Uses three-tier rendering for optimal quality:
///   1. Uniform cells (low contrast) → space with averaged bg color
///   2. Gradient cells (medium contrast) → half-block with top/bottom spatial split
///   3. High-contrast cells (text, edges) → octant with k-means 2-color quantization
pub struct OctantWidget<'a> {
    pub buf: &'a PixelBuffer,
}

/// Contrast thresholds for three-tier rendering.
/// Luminance range: 0..2550 (r*2 + g*7 + b, max = 255*10).
const UNIFORM_THRESH: u32 = 200; // ~8% — uniform color, use space
const OCTANT_THRESH: u32 = 600; // ~24% — above this, use octant; below, use half-block

impl<'a> Widget for OctantWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let src_w = self.buf.width as usize;
        let src_h = self.buf.height as usize;
        if src_w == 0 || src_h == 0 || area.width == 0 || area.height == 0 {
            return;
        }

        let dst_w = area.width as usize;
        let dst_h = area.height as usize;
        let pixels = &self.buf.pixels;
        let s2l = &*SRGB_TO_LINEAR;
        let l2s = &*LINEAR_TO_SRGB;

        // Fast path: exact 1:1 pixel mapping (canvas = cols*2 × rows*4)
        let sub_w = dst_w * 2;
        let sub_h = dst_h * 4;
        let exact = src_w == sub_w && src_h == sub_h;

        for row in 0..dst_h {
            for col in 0..dst_w {
                // Read 8 pixels (2×4 per cell)
                let mut pix = [(0u8, 0u8, 0u8); 8];
                let mut lums = [0u32; 8];
                let mut min_lum = u32::MAX;
                let mut max_lum = 0u32;

                if exact {
                    // Direct indexing — no division, no rounding error
                    let base_x = col * 2;
                    let base_y = row * 4;
                    for sr in 0..4usize {
                        let row_off = (base_y + sr) * src_w * 4;
                        for sc in 0..2usize {
                            let idx = sr * 2 + sc;
                            let i = row_off + (base_x + sc) * 4;
                            let r = pixels[i];
                            let g = pixels[i + 1];
                            let b = pixels[i + 2];
                            pix[idx] = (r, g, b);
                            let lum = r as u32 * 2 + g as u32 * 7 + b as u32;
                            lums[idx] = lum;
                            if lum < min_lum {
                                min_lum = lum;
                            }
                            if lum > max_lum {
                                max_lum = lum;
                            }
                        }
                    }
                } else {
                    // Scaled mapping with rounding
                    for sr in 0..4usize {
                        let sy = ((row * 4 + sr) * src_h + sub_h / 2) / sub_h;
                        let sy = sy.min(src_h - 1);
                        let row_off = sy * src_w * 4;
                        for sc in 0..2usize {
                            let sx = ((col * 2 + sc) * src_w + sub_w / 2) / sub_w;
                            let sx = sx.min(src_w - 1);
                            let idx = sr * 2 + sc;
                            let i = row_off + sx * 4;
                            let r = pixels[i];
                            let g = pixels[i + 1];
                            let b = pixels[i + 2];
                            pix[idx] = (r, g, b);
                            let lum = r as u32 * 2 + g as u32 * 7 + b as u32;
                            lums[idx] = lum;
                            if lum < min_lum {
                                min_lum = lum;
                            }
                            if lum > max_lum {
                                max_lum = lum;
                            }
                        }
                    }
                }

                let cell = match buf.cell_mut((area.x + col as u16, area.y + row as u16)) {
                    Some(c) => c,
                    None => continue,
                };

                let contrast = max_lum - min_lum;

                // --- Tier 1: Uniform cell → space with averaged bg ---
                if contrast < UNIFORM_THRESH {
                    let mut r_sum = 0u32;
                    let mut g_sum = 0u32;
                    let mut b_sum = 0u32;
                    for p in &pix {
                        r_sum += s2l[p.0 as usize] as u32;
                        g_sum += s2l[p.1 as usize] as u32;
                        b_sum += s2l[p.2 as usize] as u32;
                    }
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(Color::Rgb(
                        linear_to_srgb(r_sum / 8, l2s),
                        linear_to_srgb(g_sum / 8, l2s),
                        linear_to_srgb(b_sum / 8, l2s),
                    )));
                    continue;
                }

                // --- Tier 2: Moderate contrast → half-block (spatial top/bottom split) ---
                if contrast < OCTANT_THRESH {
                    // Average top 4 pixels (rows 0-1) and bottom 4 pixels (rows 2-3)
                    let mut tr = 0u32;
                    let mut tg = 0u32;
                    let mut tb = 0u32;
                    let mut br_sum = 0u32;
                    let mut bg_sum = 0u32;
                    let mut bb_sum = 0u32;
                    for idx in 0..4 {
                        tr += s2l[pix[idx].0 as usize] as u32;
                        tg += s2l[pix[idx].1 as usize] as u32;
                        tb += s2l[pix[idx].2 as usize] as u32;
                    }
                    for idx in 4..8 {
                        br_sum += s2l[pix[idx].0 as usize] as u32;
                        bg_sum += s2l[pix[idx].1 as usize] as u32;
                        bb_sum += s2l[pix[idx].2 as usize] as u32;
                    }
                    let top_r = linear_to_srgb(tr / 4, l2s);
                    let top_g = linear_to_srgb(tg / 4, l2s);
                    let top_b = linear_to_srgb(tb / 4, l2s);
                    let bot_r = linear_to_srgb(br_sum / 4, l2s);
                    let bot_g = linear_to_srgb(bg_sum / 4, l2s);
                    let bot_b = linear_to_srgb(bb_sum / 4, l2s);
                    // Check if top/bottom are nearly identical → use space
                    let dr = (top_r as i16 - bot_r as i16).unsigned_abs();
                    let dg = (top_g as i16 - bot_g as i16).unsigned_abs();
                    let db = (top_b as i16 - bot_b as i16).unsigned_abs();
                    if dr + dg + db <= 6 {
                        let r = ((top_r as u16 + bot_r as u16) / 2) as u8;
                        let g = ((top_g as u16 + bot_g as u16) / 2) as u8;
                        let b = ((top_b as u16 + bot_b as u16) / 2) as u8;
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(Color::Rgb(r, g, b)));
                    } else {
                        cell.set_char('\u{2580}'); // ▀
                        cell.set_style(
                            Style::default()
                                .fg(Color::Rgb(top_r, top_g, top_b))
                                .bg(Color::Rgb(bot_r, bot_g, bot_b)),
                        );
                    }
                    continue;
                }

                // --- Tier 3: High contrast → octant with k-means 2-color split ---

                // Find the two most distant pixels in sRGB space (seed pair for k-means)
                let mut max_dist = 0u32;
                let mut seed_a = 0usize;
                let mut seed_b = 1usize;
                for i in 0..8usize {
                    for j in (i + 1)..8usize {
                        let dr = pix[i].0 as i32 - pix[j].0 as i32;
                        let dg = pix[i].1 as i32 - pix[j].1 as i32;
                        let db = pix[i].2 as i32 - pix[j].2 as i32;
                        let d = (dr * dr + dg * dg + db * db) as u32;
                        if d > max_dist {
                            max_dist = d;
                            seed_a = i;
                            seed_b = j;
                        }
                    }
                }

                // Assign each pixel to nearest seed, accumulate in linear space
                let mut a_r = 0u32;
                let mut a_g = 0u32;
                let mut a_b = 0u32;
                let mut a_n = 0u32;
                let mut b_r = 0u32;
                let mut b_g = 0u32;
                let mut b_b = 0u32;
                let mut b_n = 0u32;
                let mut a_lum_sum = 0u32;
                let mut b_lum_sum = 0u32;
                let mut pattern: u8 = 0;

                let sa = pix[seed_a];
                let sb = pix[seed_b];

                for idx in 0..8usize {
                    let p = pix[idx];
                    // Distance to seed A vs seed B (in sRGB, squared)
                    let da_r = p.0 as i32 - sa.0 as i32;
                    let da_g = p.1 as i32 - sa.1 as i32;
                    let da_b = p.2 as i32 - sa.2 as i32;
                    let dist_a = (da_r * da_r + da_g * da_g + da_b * da_b) as u32;

                    let db_r = p.0 as i32 - sb.0 as i32;
                    let db_g = p.1 as i32 - sb.1 as i32;
                    let db_b = p.2 as i32 - sb.2 as i32;
                    let dist_b = (db_r * db_r + db_g * db_g + db_b * db_b) as u32;

                    let rl = s2l[p.0 as usize] as u32;
                    let gl = s2l[p.1 as usize] as u32;
                    let bl = s2l[p.2 as usize] as u32;

                    if dist_a <= dist_b {
                        pattern |= 1 << idx;
                        a_r += rl;
                        a_g += gl;
                        a_b += bl;
                        a_n += 1;
                        a_lum_sum += lums[idx];
                    } else {
                        b_r += rl;
                        b_g += gl;
                        b_b += bl;
                        b_n += 1;
                        b_lum_sum += lums[idx];
                    }
                }

                // Edge case: all pixels in one group
                if a_n == 0 || b_n == 0 {
                    let total_r = (a_r + b_r) / 8;
                    let total_g = (a_g + b_g) / 8;
                    let total_b = (a_b + b_b) / 8;
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(Color::Rgb(
                        linear_to_srgb(total_r, l2s),
                        linear_to_srgb(total_g, l2s),
                        linear_to_srgb(total_b, l2s),
                    )));
                    continue;
                }

                // Compute average colors for each group
                let mut fg_r = linear_to_srgb(a_r / a_n, l2s);
                let mut fg_g = linear_to_srgb(a_g / a_n, l2s);
                let mut fg_b = linear_to_srgb(a_b / a_n, l2s);
                let mut bg_r = linear_to_srgb(b_r / b_n, l2s);
                let mut bg_g = linear_to_srgb(b_g / b_n, l2s);
                let mut bg_b = linear_to_srgb(b_b / b_n, l2s);

                // Ensure fg is the brighter group (convention: bit=1 = fg = bright)
                let a_avg_lum = a_lum_sum / a_n;
                let b_avg_lum = b_lum_sum / b_n;
                if a_avg_lum < b_avg_lum {
                    // Group A is darker — swap so fg is bright, invert pattern
                    std::mem::swap(&mut fg_r, &mut bg_r);
                    std::mem::swap(&mut fg_g, &mut bg_g);
                    std::mem::swap(&mut fg_b, &mut bg_b);
                    pattern = !pattern;
                }

                cell.set_char(OCTANT_CHARS[pattern as usize]);
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(fg_r, fg_g, fg_b))
                        .bg(Color::Rgb(bg_r, bg_g, bg_b)),
                );
            }
        }
    }
}

/// Widget that renders a PixelBuffer to the terminal using half-block characters.
/// Each terminal cell represents 2 vertical pixels: the top pixel is the foreground
/// color of '▀' (U+2580), and the bottom pixel is the background color.
/// Uses gamma-correct area-averaged downsampling for accurate color reproduction.
pub struct PixelWidget<'a> {
    pub buf: &'a PixelBuffer,
}

impl<'a> Widget for PixelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let src_w = self.buf.width as usize;
        let src_h = self.buf.height as usize;
        if src_w == 0 || src_h == 0 || area.width == 0 || area.height == 0 {
            return;
        }

        let dst_w = area.width as usize;
        let dst_px_h = area.height as usize * 2; // 2 vertical pixels per cell
        let pixels = &self.buf.pixels;
        let s2l = &*SRGB_TO_LINEAR; // deref once
        let l2s = &*LINEAR_TO_SRGB;

        // Pre-compute column source ranges: for each dst col, [x_start..x_end) in src
        let col_ranges: Vec<(usize, usize)> = (0..dst_w)
            .map(|col| {
                let x0 = col * src_w / dst_w;
                let x1 = ((col + 1) * src_w / dst_w).max(x0 + 1).min(src_w);
                (x0, x1)
            })
            .collect();

        for row in 0..area.height as usize {
            // Top pixel: rows [yt0..yt1), bottom pixel: rows [yb0..yb1)
            let yt0 = (row * 2) * src_h / dst_px_h;
            let yt1 = ((row * 2 + 1) * src_h / dst_px_h).max(yt0 + 1).min(src_h);
            let yb0 = (row * 2 + 1) * src_h / dst_px_h;
            let yb1 = ((row * 2 + 2) * src_h / dst_px_h).max(yb0 + 1).min(src_h);

            for col in 0..dst_w {
                let (x0, x1) = col_ranges[col];

                let top = avg_area_gamma(pixels, src_w, x0, yt0, x1, yt1, s2l, l2s);
                let bot = avg_area_gamma(pixels, src_w, x0, yb0, x1, yb1, s2l, l2s);

                if let Some(cell) = buf.cell_mut((area.x + col as u16, area.y + row as u16)) {
                    let dr = (top.0 as i16 - bot.0 as i16).unsigned_abs();
                    let dg = (top.1 as i16 - bot.1 as i16).unsigned_abs();
                    let db = (top.2 as i16 - bot.2 as i16).unsigned_abs();
                    if dr + dg + db <= 6 {
                        let r = ((top.0 as u16 + bot.0 as u16) / 2) as u8;
                        let g = ((top.1 as u16 + bot.1 as u16) / 2) as u8;
                        let b = ((top.2 as u16 + bot.2 as u16) / 2) as u8;
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(Color::Rgb(r, g, b)));
                    } else {
                        cell.set_char('\u{2580}'); // ▀
                        cell.set_style(
                            Style::default()
                                .fg(Color::Rgb(top.0, top.1, top.2))
                                .bg(Color::Rgb(bot.0, bot.1, bot.2)),
                        );
                    }
                }
            }
        }
    }
}

/// Gamma-correct area average of all RGBA pixels in [x0..x1) x [y0..y1).
#[inline]
fn avg_area_gamma(
    pixels: &[u8],
    src_w: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    s2l: &[u16; 256],
    l2s: &[u8; 1024],
) -> (u8, u8, u8) {
    let count = (x1 - x0) * (y1 - y0);
    if count <= 1 {
        let i = (y0 * src_w + x0) * 4;
        return (pixels[i], pixels[i + 1], pixels[i + 2]);
    }
    let mut r_sum: u32 = 0;
    let mut g_sum: u32 = 0;
    let mut b_sum: u32 = 0;

    for y in y0..y1 {
        let row_base = y * src_w * 4;
        for x in x0..x1 {
            let i = row_base + x * 4;
            r_sum += s2l[pixels[i] as usize] as u32;
            g_sum += s2l[pixels[i + 1] as usize] as u32;
            b_sum += s2l[pixels[i + 2] as usize] as u32;
        }
    }

    let n = count as u32;
    (
        linear_to_srgb(r_sum / n, l2s),
        linear_to_srgb(g_sum / n, l2s),
        linear_to_srgb(b_sum / n, l2s),
    )
}

// ══════════════════════════════════════════════════════════════════════════════
// Sixel rendering — pixel-level graphics via DCS escape sequences
// ══════════════════════════════════════════════════════════════════════════════

/// Encode a PixelBuffer as a Sixel escape sequence for direct terminal output.
/// Sixel renders actual pixels (not character-cell approximations), giving much
/// higher fidelity than octant or half-block modes.
///
/// `target_w` / `target_h` specify the output Sixel image dimensions.
/// If they differ from the pixel buffer size, nearest-neighbor upscaling is used.
/// Pass (0, 0) to output at the pixel buffer's native resolution.
pub fn encode_sixel(pb: &PixelBuffer, target_w: u32, target_h: u32) -> Vec<u8> {
    let src_w = pb.width as u32;
    let src_h = pb.height as u32;
    if src_w == 0 || src_h == 0 {
        return Vec::new();
    }

    SIXEL_BUF.with_borrow_mut(|sb| encode_sixel_inner(pb, target_w, target_h, sb))
}

fn encode_sixel_inner(
    pb: &PixelBuffer,
    target_w: u32,
    target_h: u32,
    sb: &mut SixelBuffers,
) -> Vec<u8> {
    let src_w = pb.width as u32;
    let src_h = pb.height as u32;

    // Output dimensions: use target if provided, otherwise native
    let out_w = if target_w > 0 { target_w } else { src_w } as usize;
    let out_h = if target_h > 0 { target_h } else { src_h } as usize;

    let pixels = &pb.pixels;
    let n = (src_w * src_h) as usize;

    // Step 1: Quantize original pixels to ≤256 colors (reuses histogram buffers)
    let _t0 = std::time::Instant::now();
    let palette = quantize_median_cut(pixels, n, 256, sb);
    let _t_quant = _t0.elapsed();

    // Step 2: Map source pixels to palette indices (reuse indices buffer)
    sb.indices.clear();
    if sb.indices.capacity() < n {
        sb.indices.reserve(n - sb.indices.capacity());
    }
    for i in 0..n {
        let off = i * 4;
        let key = ((pixels[off] as usize) >> 3) << 10
            | ((pixels[off + 1] as usize) >> 3) << 5
            | ((pixels[off + 2] as usize) >> 3);
        sb.indices.push(sb.lut[key]);
    }
    let _t_index = _t0.elapsed();

    let n_colors = palette.len();

    // Work in source coordinates for bands, use exact nearest-neighbor
    // resampling for any scale ratio (not just integer multiples).
    let sw = src_w as usize;
    let sh = src_h as usize;
    let n_bands = (out_h + 5) / 6;

    // Precompute per-source-column output width for X resampling (cached across frames).
    if sb.col_widths_key != (sw, out_w) {
        sb.col_widths.clear();
        sb.col_widths
            .reserve(sw.saturating_sub(sb.col_widths.capacity()));
        for x in 0..sw {
            let start = x * out_w / sw;
            let end = (x + 1) * out_w / sw;
            sb.col_widths.push((end - start) as u32);
        }
        sb.col_widths_key = (sw, out_w);
    }

    // Reuse output buffer
    sb.output.clear();
    let est_size = n_colors * 30 + n_bands * n_colors * (sw / 4 + 10) + 256;
    if sb.output.capacity() < est_size {
        sb.output.reserve(est_size - sb.output.capacity());
    }

    // DCS header with raster attributes
    sb.output.extend_from_slice(b"\x1bPq\"1;1;");
    write_u32(&mut sb.output, out_w as u32);
    sb.output.push(b';');
    write_u32(&mut sb.output, out_h as u32);

    // Palette definitions
    for (i, color) in palette.iter().enumerate() {
        sb.output.push(b'#');
        write_u32(&mut sb.output, i as u32);
        sb.output.extend_from_slice(b";2;");
        write_u32(&mut sb.output, (color[0] as u32 * 100 + 127) / 255);
        sb.output.push(b';');
        write_u32(&mut sb.output, (color[1] as u32 * 100 + 127) / 255);
        sb.output.push(b';');
        write_u32(&mut sb.output, (color[2] as u32 * 100 + 127) / 255);
    }

    // Resize band buffers if needed, then fill(0) per band
    let cb_needed = n_colors * sw;
    sb.color_bits.resize(cb_needed, 0);
    sb.used.resize(n_colors, false);

    for band in 0..n_bands {
        let y0 = band * 6; // in output rows

        // Map each of the 6 output rows in this band to a source row
        let band_h = (out_h - y0).min(6);
        let mut src_ys = [0usize; 6];
        for dy in 0..band_h {
            // Nearest-neighbor Y: output row → source row
            src_ys[dy] = (((y0 + dy) * sh) / out_h).min(sh - 1);
        }

        // Clear reused buffers
        sb.color_bits[..cb_needed].fill(0);
        sb.used[..n_colors].fill(false);
        sb.active_colors.clear();

        // Build sixel bit patterns at SOURCE resolution, track active colors
        for x in 0..sw {
            for dy in 0..band_h {
                let ci = sb.indices[src_ys[dy] * sw + x] as usize;
                sb.color_bits[ci * sw + x] |= 1 << dy;
                if !sb.used[ci] {
                    sb.used[ci] = true;
                    sb.active_colors.push(ci);
                }
            }
        }

        // Encode only active colors — skip the ~220 unused per band
        for ci_idx in 0..sb.active_colors.len() {
            let ci = sb.active_colors[ci_idx];
            sb.output.push(b'#');
            write_u32(&mut sb.output, ci as u32);

            let bits_start = ci * sw;
            let mut prev_sixel: u8 = 0;
            let mut run_len: u32 = 0;

            for x in 0..sw {
                let w = sb.col_widths[x];
                if w == 0 {
                    continue;
                } // downscale: skip
                let sixel_ch = 63 + sb.color_bits[bits_start + x];

                if run_len > 0 && sixel_ch == prev_sixel {
                    run_len += w;
                } else {
                    if run_len > 0 {
                        flush_rle(&mut sb.output, prev_sixel, run_len);
                    }
                    prev_sixel = sixel_ch;
                    run_len = w;
                }
            }
            if run_len > 0 {
                flush_rle(&mut sb.output, prev_sixel, run_len);
            }

            sb.output.push(b'$');
        }

        if band < n_bands - 1 {
            sb.output.push(b'-');
        }
    }

    // String terminator
    sb.output.extend_from_slice(b"\x1b\\");

    // Profile logging (every 120th call)
    static ENCODE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let call_n = ENCODE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if call_n % 120 == 0 {
        let _t_total = _t0.elapsed();
        eprintln!("[ENCODE-PROF] quant={:.1}ms index={:.1}ms bands={:.1}ms total={:.1}ms pixels={}K colors={} bands={} src={}x{}",
            _t_quant.as_secs_f64() * 1000.0,
            (_t_index - _t_quant).as_secs_f64() * 1000.0,
            (_t_total - _t_index).as_secs_f64() * 1000.0,
            _t_total.as_secs_f64() * 1000.0,
            n / 1024, n_colors, n_bands, src_w, src_h);
    }

    // Return the output by swapping with an empty Vec (avoids clone, reuses buffer next frame)
    std::mem::take(&mut sb.output)
}

/// Write a u32 as decimal ASCII digits directly into a Vec<u8>.
/// Avoids format_args!/std::fmt overhead (~150ns per call × 180K calls/frame).
#[inline(always)]
fn write_u32(out: &mut Vec<u8>, n: u32) {
    if n < 10 {
        out.push(b'0' + n as u8);
        return;
    }
    if n < 100 {
        out.push(b'0' + (n / 10) as u8);
        out.push(b'0' + (n % 10) as u8);
        return;
    }
    let start = out.len();
    let mut v = n;
    while v > 0 {
        out.push(b'0' + (v % 10) as u8);
        v /= 10;
    }
    out[start..].reverse();
}

/// Flush an RLE run to the output buffer.
#[inline]
fn flush_rle(out: &mut Vec<u8>, ch: u8, count: u32) {
    if count <= 3 {
        for _ in 0..count {
            out.push(ch);
        }
    } else {
        out.push(b'!');
        write_u32(out, count);
        out.push(ch);
    }
}

/// Median-cut color quantization: reduces image to ≤max_colors colors.
/// Returns palette; LUT is written into qb.lut (maps 5-bit RGB keys to palette indices).
fn quantize_median_cut(
    pixels: &[u8],
    n: usize,
    max_colors: usize,
    qb: &mut SixelBuffers,
) -> Vec<[u8; 3]> {
    // Clear histogram buffers (reused across frames)
    qb.counts.fill(0);
    qb.r_sums.fill(0);
    qb.g_sums.fill(0);
    qb.b_sums.fill(0);
    qb.lut.fill(0);

    for i in 0..n {
        let off = i * 4;
        let r = pixels[off];
        let g = pixels[off + 1];
        let b = pixels[off + 2];
        let key = ((r as usize) >> 3) << 10 | ((g as usize) >> 3) << 5 | ((b as usize) >> 3);
        qb.counts[key] += 1;
        qb.r_sums[key] += r as u64;
        qb.g_sums[key] += g as u64;
        qb.b_sums[key] += b as u64;
    }

    // Collect non-zero histogram entries
    let active: Vec<usize> = (0..32768).filter(|&k| qb.counts[k] > 0).collect();

    // If ≤max_colors unique colors, no quantization needed
    if active.len() <= max_colors {
        let mut palette = Vec::with_capacity(active.len());
        for (pi, &key) in active.iter().enumerate() {
            let c = qb.counts[key] as u64;
            palette.push([
                (qb.r_sums[key] / c) as u8,
                (qb.g_sums[key] / c) as u8,
                (qb.b_sums[key] / c) as u8,
            ]);
            qb.lut[key] = pi as u8;
        }
        return palette;
    }

    // Iterative median cut: split boxes until we have max_colors
    let mut boxes: Vec<Vec<usize>> = vec![active];

    while boxes.len() < max_colors {
        // Find box with largest range in any channel (weighted: G×1.2 for perceptual)
        let mut best_box = usize::MAX;
        let mut best_range = 0u32;
        let mut best_channel = 0u8;

        for (bi, bx) in boxes.iter().enumerate() {
            if bx.len() <= 1 {
                continue;
            }
            let (r_min, r_max, g_min, g_max, b_min, b_max) = box_bounds(bx);
            let r_range = (r_max - r_min) as u32;
            let g_range = ((g_max - g_min) as u32 * 6) / 5; // Boost green (perceptual)
            let b_range = (b_max - b_min) as u32;

            let (range, ch) = if r_range >= g_range && r_range >= b_range {
                (r_range, 0u8)
            } else if g_range >= b_range {
                (g_range, 1u8)
            } else {
                (b_range, 2u8)
            };

            if range > best_range {
                best_range = range;
                best_box = bi;
                best_channel = ch;
            }
        }

        if best_box == usize::MAX || best_range == 0 {
            break;
        }

        // Sort the box by the selected channel and split at pixel-count median
        let mut bx = boxes.swap_remove(best_box);
        bx.sort_unstable_by_key(|&key| match best_channel {
            0 => (key >> 10) as u8,
            1 => ((key >> 5) & 0x1F) as u8,
            _ => (key & 0x1F) as u8,
        });

        let total_pixels: u64 = bx.iter().map(|&k| qb.counts[k] as u64).sum();
        let mut cumulative = 0u64;
        let mut split_at = bx.len() / 2;
        for (i, &key) in bx.iter().enumerate() {
            cumulative += qb.counts[key] as u64;
            if cumulative >= total_pixels / 2 {
                split_at = (i + 1).max(1).min(bx.len() - 1);
                break;
            }
        }

        let right = bx.split_off(split_at);
        boxes.push(bx);
        boxes.push(right);
    }

    // Compute palette colors and build LUT (lut already cleared above)
    let mut palette = Vec::with_capacity(boxes.len());

    for (pi, bx) in boxes.iter().enumerate() {
        let mut total_count = 0u64;
        let mut total_r = 0u64;
        let mut total_g = 0u64;
        let mut total_b = 0u64;

        for &key in bx {
            let c = qb.counts[key] as u64;
            total_count += c;
            total_r += qb.r_sums[key];
            total_g += qb.g_sums[key];
            total_b += qb.b_sums[key];
            qb.lut[key] = pi as u8;
        }

        if total_count > 0 {
            palette.push([
                (total_r / total_count) as u8,
                (total_g / total_count) as u8,
                (total_b / total_count) as u8,
            ]);
        } else {
            palette.push([0, 0, 0]);
        }
    }

    palette
}

/// Compute the 5-bit RGB bounds of a color box.
fn box_bounds(bx: &[usize]) -> (u8, u8, u8, u8, u8, u8) {
    let mut r_min = 31u8;
    let mut r_max = 0u8;
    let mut g_min = 31u8;
    let mut g_max = 0u8;
    let mut b_min = 31u8;
    let mut b_max = 0u8;

    for &key in bx {
        let r = (key >> 10) as u8;
        let g = ((key >> 5) & 0x1F) as u8;
        let b = (key & 0x1F) as u8;
        if r < r_min {
            r_min = r;
        }
        if r > r_max {
            r_max = r;
        }
        if g < g_min {
            g_min = g;
        }
        if g > g_max {
            g_max = g;
        }
        if b < b_min {
            b_min = b;
        }
        if b > b_max {
            b_max = b;
        }
    }

    (r_min, r_max, g_min, g_max, b_min, b_max)
}
