// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

pub(super) fn draw_line_bresenham(
    pb: &mut PixelBuffer,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        if cx >= 0 && cy >= 0 {
            pb.set_pixel(cx as u32, cy as u32, color[0], color[1], color[2], color[3]);
        }
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

/// Draw a line with thickness. For width <= 1.5, falls back to Bresenham.
/// For thicker lines, draws a filled rectangle along the line direction.
pub(super) fn draw_thick_line(
    pb: &mut PixelBuffer,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    color: [u8; 4],
) {
    if width <= 1.5 {
        draw_line_bresenham(pb, x0 as i32, y0 as i32, x1 as i32, y1 as i32, color);
        return;
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }
    // Normal perpendicular to line direction
    let half_w = width * 0.5;
    let nx = -dy / len * half_w;
    let ny = dx / len * half_w;
    // Four corners of the thick line rectangle
    let vertices = [
        (x0 + nx, y0 + ny),
        (x0 - nx, y0 - ny),
        (x1 - nx, y1 - ny),
        (x1 + nx, y1 + ny),
    ];
    fill_polygon(pb, &vertices, color);
}

/// Draw a filled ellipse using the midpoint algorithm with horizontal spans.
pub(super) fn draw_filled_ellipse(
    pb: &mut PixelBuffer,
    cx: i32,
    cy: i32,
    rx: i32,
    ry: i32,
    color: [u8; 4],
) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    let rx_f = rx as f32;
    let ry_f = ry as f32;

    for dy in -ry..=ry {
        let ny = dy as f32 / ry_f;
        let ny2 = ny * ny;
        if ny2 >= 1.0 {
            continue;
        }
        // Exact edge x at this y: x_edge = rx * sqrt(1 - ny^2)
        let x_edge = rx_f * (1.0 - ny2).sqrt();
        let x_span = x_edge as i32;
        let row_y = cy + dy;
        // Interior fill (fully inside)
        if x_span > 0 {
            pb.fill_rect(cx - x_span + 1, row_y, (x_span - 1) * 2, 1, color);
        }
        // Anti-alias edge pixels: compute coverage from ellipse distance
        for &dx in &[-(x_span), x_span] {
            let nx = dx as f32 / rx_f;
            let d = nx * nx + ny2;
            let cov = if d <= 0.85 {
                1.0
            } else if d >= 1.15 {
                0.0
            } else {
                let t = ((d - 0.85) * (1.0 / 0.3)).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            };
            if cov > 0.01 {
                let a = (color[3] as f32 * cov) as u8;
                let px = cx + dx;
                if px >= 0 && row_y >= 0 {
                    pb.set_pixel(px as u32, row_y as u32, color[0], color[1], color[2], a);
                }
            }
        }
        // Also AA the pixel just outside the edge
        for &dx in &[-(x_span + 1), x_span + 1] {
            let nx = dx as f32 / rx_f;
            let d = nx * nx + ny2;
            if d < 1.15 {
                let cov = if d <= 0.85 {
                    1.0
                } else {
                    let t = ((d - 0.85) * (1.0 / 0.3)).clamp(0.0, 1.0);
                    1.0 - t * t * (3.0 - 2.0 * t)
                };
                if cov > 0.01 {
                    let a = (color[3] as f32 * cov) as u8;
                    let px = cx + dx;
                    if px >= 0 && row_y >= 0 {
                        pb.set_pixel(px as u32, row_y as u32, color[0], color[1], color[2], a);
                    }
                }
            }
        }
    }
}

/// Fill a polygon using scanline with even-odd rule.
/// For convex/simple polygons (like Balatro's rounded rects), this works directly.
/// For complex polygons with a center vertex (triangle fan format), the even-odd
/// rule still produces correct results for the perimeter shape.
pub(super) fn fill_polygon(pb: &mut PixelBuffer, vertices: &[(f32, f32)], color: [u8; 4]) -> bool {
    if vertices.len() < 3 {
        return false;
    }

    // Detect triangle-fan format: if vertex[0] is roughly at the centroid
    // (inside the bounding box of the remaining vertices), use perimeter only.
    // This avoids seam artifacts from triangle decomposition.
    let perimeter = if vertices.len() > 6 {
        let (cx, cy) = vertices[0];
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for &(x, y) in &vertices[1..] {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        if cx > min_x && cx < max_x && cy > min_y && cy < max_y {
            &vertices[1..] // Skip center vertex, use perimeter only
        } else {
            vertices
        }
    } else {
        vertices
    };

    // Scanline fill with even-odd rule
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for &(_, y) in perimeter {
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }

    let mut y_start = (min_y.floor() as i32).max(0);
    let mut y_end = (max_y.ceil() as i32).min(pb.height as i32 - 1);
    if let Some((_, sy, _, sh)) = pb.scissor {
        y_start = y_start.max(sy);
        y_end = y_end.min(sy.saturating_add(sh as i32).saturating_sub(1));
    }
    if y_start > y_end {
        return false;
    }

    if fill_simple_polygon(pb, perimeter, color, y_start, y_end) {
        return true;
    }

    fill_polygon_scanline(pb, perimeter, color, y_start, y_end);
    false
}

const MAX_SIMPLE_POLYGON_ROWS: usize = 240;

#[derive(Clone, Copy)]
struct SimplePolygonSpan {
    left: f32,
    right: f32,
    intersections: u8,
}

impl SimplePolygonSpan {
    const EMPTY: Self = Self {
        left: f32::MAX,
        right: f32::MIN,
        intersections: 0,
    };
}

/// Fast path for polygons with at most two edge intersections per scanline.
/// Each edge contributes only to the rows it crosses, avoiding a full edge scan
/// for every row while preserving the reference rasterizer's crossing equation.
fn fill_simple_polygon(
    pb: &mut PixelBuffer,
    perimeter: &[(f32, f32)],
    color: [u8; 4],
    y_start: i32,
    y_end: i32,
) -> bool {
    let row_count = (y_end - y_start + 1) as usize;
    if row_count > MAX_SIMPLE_POLYGON_ROWS {
        return false;
    }

    let mut spans = [SimplePolygonSpan::EMPTY; MAX_SIMPLE_POLYGON_ROWS];

    for i in 0..perimeter.len() {
        let (x0, y0) = perimeter[i];
        let (x1, y1) = perimeter[(i + 1) % perimeter.len()];

        if y0 == y1 {
            continue;
        }

        let edge_min_y = y0.min(y1);
        let edge_max_y = y0.max(y1);
        let first_y = ((edge_min_y - 0.5).ceil() as i32).max(y_start);
        let end_y = ((edge_max_y - 0.5).ceil() as i32).min(y_end + 1);
        let inverse_height = 1.0 / (y1 - y0);

        for y in first_y..end_y {
            let span = &mut spans[(y - y_start) as usize];
            if span.intersections == 2 {
                return false;
            }
            let yf = y as f32 + 0.5;
            let t = (yf - y0) * inverse_height;
            let x = x0 + t * (x1 - x0);
            span.left = span.left.min(x);
            span.right = span.right.max(x);
            span.intersections += 1;
        }
    }

    if spans[..row_count]
        .iter()
        .any(|span| span.intersections == 1)
    {
        return false;
    }

    for (row, span) in spans[..row_count].iter().enumerate() {
        if span.intersections == 2 {
            fill_polygon_span(pb, y_start + row as i32, span.left, span.right, color);
        }
    }
    true
}

fn fill_polygon_scanline(
    pb: &mut PixelBuffer,
    perimeter: &[(f32, f32)],
    color: [u8; 4],
    y_start: i32,
    y_end: i32,
) {
    // A Balatro panel has about thirty edges. Reuse one intersection buffer for
    // the whole polygon instead of allocating it again for every scanline.
    let mut intersections = Vec::with_capacity(perimeter.len());

    for y in y_start..=y_end {
        let yf = y as f32 + 0.5;
        intersections.clear();

        for i in 0..perimeter.len() {
            let j = (i + 1) % perimeter.len();
            let (x0, y0) = perimeter[i];
            let (x1, y1) = perimeter[j];

            if (y0 <= yf && y1 > yf) || (y1 <= yf && y0 > yf) {
                let t = (yf - y0) / (y1 - y0);
                intersections.push(x0 + t * (x1 - x0));
            }
        }

        if intersections.len() == 2 {
            if intersections[0] > intersections[1] {
                intersections.swap(0, 1);
            }
        } else {
            intersections
                .sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        }

        for pair in intersections.chunks(2) {
            if pair.len() == 2 {
                fill_polygon_span(pb, y, pair[0], pair[1], color);
            }
        }
    }
}

#[inline]
fn fill_polygon_span(pb: &mut PixelBuffer, y: i32, left: f32, right: f32, color: [u8; 4]) {
    let x_start = left.floor() as i32;
    let x_end = right.ceil() as i32;
    if x_end <= x_start {
        return;
    }

    let left_cov = (1.0 - (left - left.floor())).clamp(0.0, 1.0);
    if left_cov > 0.01 && left_cov < 0.99 {
        let a = (color[3] as f32 * left_cov) as u8;
        if x_start >= 0 && y >= 0 {
            pb.set_pixel(x_start as u32, y as u32, color[0], color[1], color[2], a);
        }
        if x_end - x_start - 1 > 0 {
            let right_cov = (right - right.floor()).clamp(0.0, 1.0);
            let r_end = x_end - 1;
            if right_cov > 0.01 && right_cov < 0.99 && r_end > x_start + 1 {
                pb.fill_rect(x_start + 1, y, r_end - x_start - 1, 1, color);
                let a2 = (color[3] as f32 * right_cov) as u8;
                if r_end >= 0 {
                    pb.set_pixel(r_end as u32, y as u32, color[0], color[1], color[2], a2);
                }
            } else {
                pb.fill_rect(x_start + 1, y, x_end - x_start - 1, 1, color);
            }
        }
    } else {
        let right_cov = (right - right.floor()).clamp(0.0, 1.0);
        let r_end = x_end - 1;
        if right_cov > 0.01 && right_cov < 0.99 && r_end > x_start {
            pb.fill_rect(x_start, y, r_end - x_start, 1, color);
            let a2 = (color[3] as f32 * right_cov) as u8;
            if r_end >= 0 {
                pb.set_pixel(r_end as u32, y as u32, color[0], color[1], color[2], a2);
            }
        } else {
            pb.fill_rect(x_start, y, x_end - x_start, 1, color);
        }
    }
}

/// Draw a stroked (outline) ellipse with anti-aliased edges using distance field.
pub(super) fn draw_stroke_ellipse(
    pb: &mut PixelBuffer,
    cx: i32,
    cy: i32,
    rx: i32,
    ry: i32,
    color: [u8; 4],
) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    let rx_f = rx as f32;
    let ry_f = ry as f32;
    // Stroke is 1 pixel wide; check band around ellipse
    for dy in -(ry + 1)..=(ry + 1) {
        let ny = dy as f32 / ry_f;
        let row_y = cy + dy;
        if row_y < 0 || row_y >= pb.height as i32 {
            continue;
        }
        // Find approximate x range where ellipse passes
        let ny2 = ny * ny;
        if ny2 > 1.3 {
            continue;
        }
        let x_edge = if ny2 < 1.0 {
            rx_f * (1.0 - ny2).sqrt()
        } else {
            0.0
        };
        let x_lo = (x_edge - 1.5).max(0.0) as i32;
        let x_hi = (x_edge + 1.5) as i32 + 1;
        for sign in &[-1i32, 1] {
            for dx_abs in x_lo..=x_hi {
                let dx = dx_abs * sign;
                let px = cx + dx;
                if px < 0 || px >= pb.width as i32 {
                    continue;
                }
                let nx = dx as f32 / rx_f;
                let d = nx * nx + ny2;
                // d = 1.0 is the ellipse. We want pixels near d=1.0 to be bright.
                let dist_from_edge = (d - 1.0).abs();
                if dist_from_edge < 0.15 {
                    pb.set_pixel(
                        px as u32,
                        row_y as u32,
                        color[0],
                        color[1],
                        color[2],
                        color[3],
                    );
                } else if dist_from_edge < 0.3 {
                    let t = ((dist_from_edge - 0.15) / 0.15).clamp(0.0, 1.0);
                    let cov = 1.0 - t * t * (3.0 - 2.0 * t);
                    let a = (color[3] as f32 * cov) as u8;
                    pb.set_pixel(px as u32, row_y as u32, color[0], color[1], color[2], a);
                }
            }
        }
    }
}

#[cfg(test)]
mod polygon_tests {
    use super::*;

    #[test]
    fn simple_polygon_fast_path_matches_generic_scanline_output() {
        let vertices = [
            (8.0, 4.0),
            (28.0, 4.0),
            (28.0, 6.0),
            (32.0, 6.0),
            (32.0, 24.0),
            (28.0, 24.0),
            (28.0, 28.0),
            (8.0, 28.0),
            (8.0, 24.0),
            (4.0, 24.0),
            (4.0, 6.0),
            (8.0, 6.0),
        ];
        let color = [92, 141, 204, 173];
        let mut fast = PixelBuffer::new(40, 36);
        let mut generic = PixelBuffer::new(40, 36);

        assert!(fill_simple_polygon(&mut fast, &vertices, color, 0, 35));
        fill_polygon_scanline(&mut generic, &vertices, color, 0, 35);

        assert_eq!(fast.pixels, generic.pixels);
    }

    #[test]
    fn simple_polygon_fast_path_matches_a_slightly_rotated_panel() {
        let vertices = [
            (8.0, 4.0),
            (28.0, 4.0),
            (28.0, 6.0),
            (32.0, 6.0),
            (32.0, 24.0),
            (28.0, 24.0),
            (28.0, 28.0),
            (8.0, 28.0),
            (8.0, 24.0),
            (4.0, 24.0),
            (4.0, 6.0),
            (8.0, 6.0),
        ];
        let angle = 0.007_f32;
        let (sine, cosine) = angle.sin_cos();
        let rotated: Vec<_> = vertices
            .iter()
            .map(|&(x, y)| {
                let x = x - 18.0;
                let y = y - 16.0;
                (x * cosine - y * sine + 18.0, x * sine + y * cosine + 16.0)
            })
            .collect();
        let color = [92, 141, 204, 173];
        let mut fast = PixelBuffer::new(40, 36);
        let mut generic = PixelBuffer::new(40, 36);

        assert!(fill_simple_polygon(&mut fast, &rotated, color, 0, 35));
        fill_polygon_scanline(&mut generic, &rotated, color, 0, 35);

        assert_eq!(fast.pixels, generic.pixels);
    }

    #[test]
    fn simple_polygon_fast_path_matches_rotated_subpixel_panels() {
        let vertices = [
            (8.0, 4.0),
            (28.0, 4.0),
            (28.0, 6.0),
            (32.0, 6.0),
            (32.0, 24.0),
            (28.0, 24.0),
            (28.0, 28.0),
            (8.0, 28.0),
            (8.0, 24.0),
            (4.0, 24.0),
            (4.0, 6.0),
            (8.0, 6.0),
        ];
        let color = [92, 141, 204, 173];

        for angle in [-0.025_f32, -0.009, -0.001, 0.0, 0.001, 0.009, 0.025] {
            let (sine, cosine) = angle.sin_cos();
            for (offset_x, offset_y) in [(0.0, 0.0), (0.17, 0.31), (-0.23, 0.09)] {
                let rotated: Vec<_> = vertices
                    .iter()
                    .map(|&(x, y)| {
                        let x = x - 18.0;
                        let y = y - 16.0;
                        (
                            x * cosine - y * sine + 18.0 + offset_x,
                            x * sine + y * cosine + 16.0 + offset_y,
                        )
                    })
                    .collect();
                let mut fast = PixelBuffer::new(48, 44);
                let mut generic = PixelBuffer::new(48, 44);

                assert!(fill_simple_polygon(&mut fast, &rotated, color, 0, 43));
                fill_polygon_scanline(&mut generic, &rotated, color, 0, 43);

                assert_eq!(fast.pixels, generic.pixels, "angle={angle}");
            }
        }
    }

    #[test]
    fn simple_polygon_fast_path_rejects_shapes_with_multiple_spans() {
        let vertices = [
            (4.0, 4.0),
            (28.0, 4.0),
            (28.0, 28.0),
            (20.0, 28.0),
            (20.0, 12.0),
            (12.0, 12.0),
            (12.0, 28.0),
            (4.0, 28.0),
        ];
        let mut target = PixelBuffer::new(32, 32);

        assert!(!fill_simple_polygon(
            &mut target,
            &vertices,
            [255, 255, 255, 255],
            0,
            31
        ));
    }

    #[test]
    fn batched_draw_transform_matches_the_original_sequence() {
        let mut original = Transform::default();
        original.translate(3.25, -1.5);
        let mut expected = original.clone();
        expected.scale(4.0, 4.0);
        expected.translate(12.5, 7.25);
        expected.rotate(0.17);
        expected.translate(-2.75, -3.5);
        expected.scale(0.85, 0.85);

        let actual = prepare_draw_transform(original, 4.0, 12.5, 7.25, 0.17, -2.75, -3.5, 0.85);

        assert_eq!(actual.a.to_bits(), expected.a.to_bits());
        assert_eq!(actual.b.to_bits(), expected.b.to_bits());
        assert_eq!(actual.c.to_bits(), expected.c.to_bits());
        assert_eq!(actual.d.to_bits(), expected.d.to_bits());
        assert_eq!(actual.tx.to_bits(), expected.tx.to_bits());
        assert_eq!(actual.ty.to_bits(), expected.ty.to_bits());
    }

    #[test]
    fn card_shaders_are_recognized_from_balatro_paths() {
        assert_eq!(card_shader_for_source("resources/shaders/foil.fs"), 3);
        assert_eq!(card_shader_for_source("resources\\shaders\\holo.fs"), 4);
        assert_eq!(card_shader_for_source("resources/shaders/polychrome.fs"), 5);
        assert_eq!(
            card_shader_for_source("resources/shaders/negative_shine.fs"),
            10
        );
    }

    #[test]
    fn card_shaders_are_recognized_from_source_text() {
        assert_eq!(
            card_shader_for_source("extern vec2 foil;\nvec4 effect() {}"),
            3
        );
        assert_eq!(
            card_shader_for_source("extern vec2 hologram;\nvec4 effect() {}"),
            9
        );
        assert_eq!(card_shader_for_source("vec4 effect() {}"), 0);
    }
}
