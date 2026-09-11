use crate::pixel_buffer::PixelBuffer;

pub fn draw(buffer: &mut PixelBuffer, bounds: [i32; 4], time: f32, opacity: f32) {
    draw_region(buffer, bounds, time, opacity, true);
}

fn draw_region(buffer: &mut PixelBuffer, bounds: [i32; 4], time: f32, opacity: f32, crop: bool) {
    if time <= 2.5 || opacity <= 0.0 {
        return;
    }
    let width = buffer.width as f32;
    let height = buffer.height as f32;
    let diagonal = width.hypot(height);
    let pixel_size = diagonal / 700.0;
    let first = (time - 2.5).sqrt();
    let second = if time > 11.0 {
        (time - 11.0).powi(2)
    } else {
        0.0
    };
    let mut x0 = bounds[0].clamp(0, buffer.width as i32);
    let mut y0 = bounds[1].clamp(0, buffer.height as i32);
    let mut x1 = bounds[2].clamp(0, buffer.width as i32);
    let mut y1 = bounds[3].clamp(0, buffer.height as i32);
    if crop {
        // Both radial terms are zero outside this radius. Keep a pixel-grid margin.
        let radius = (first / 60.0).max(second / 5.0) * diagonal + pixel_size + 1.0;
        x0 = x0.max((width * 0.5 - radius).floor() as i32);
        y0 = y0.max((height * 0.5 - radius).floor() as i32);
        x1 = x1.min((width * 0.5 + radius).ceil() as i32);
        y1 = y1.min((height * 0.5 + radius).ceil() as i32);
    }
    for y in y0..y1 {
        let uy = (((y as f32 + 0.5) / pixel_size).floor() * pixel_size - height * 0.5) / diagonal;
        for x in x0..x1 {
            let ux =
                (((x as f32 + 0.5) / pixel_size).floor() * pixel_size - width * 0.5) / diagonal;
            let radius = ux.hypot(uy);
            let white =
                ((first - 60.0 * radius).max(0.0) + (second - 5.0 * radius).max(0.0)).min(1.0);
            let alpha = (opacity * white * 255.0).clamp(0.0, 255.0) as u8;
            if alpha > 0 {
                buffer.blend_at(
                    (y as usize * buffer.width as usize + x as usize) * 4,
                    255,
                    255,
                    255,
                    alpha,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_flash_matches_the_full_screen_pass() {
        for (w, h) in [(640, 480), (321, 239), (97, 151)] {
            for time in [2.5001, 3.0, 5.0, 10.9, 11.0, 11.2, 11.5, 12.0, 13.0, 15.0] {
                for opacity in [0.1, 0.5, 1.0] {
                    for bounds in [
                        [-20, -10, w + 10, h + 20],
                        [w / 3, h / 3, w * 2 / 3, h * 2 / 3],
                    ] {
                        let mut reference = PixelBuffer::new(w as u32, h as u32);
                        for (n, byte) in reference.pixels.iter_mut().enumerate() {
                            *byte = (n % 251) as u8;
                        }
                        let mut bounded = PixelBuffer::new(w as u32, h as u32);
                        bounded.pixels.copy_from_slice(&reference.pixels);
                        draw_region(&mut reference, bounds, time, opacity, false);
                        draw(&mut bounded, bounds, time, opacity);
                        assert_eq!(
                            bounded.pixels, reference.pixels,
                            "{w}x{h} time={time} opacity={opacity}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn flash_starts_transparent_and_expands_from_the_centre() {
        let mut buffer = PixelBuffer::new(640, 480);
        draw(&mut buffer, [-10, -10, 650, 490], 2.5, 1.0);
        assert!(buffer.pixels.iter().all(|&v| v == 0));
        draw(&mut buffer, [0, 0, 640, 480], 3.5, 1.0);
        assert!(buffer.pixels[(240 * 640 + 320) * 4] > 230);
        assert_eq!(buffer.pixels[0], 0);
        draw(&mut buffer, [0, 0, 640, 480], 15.0, 1.0);
        assert!(buffer
            .pixels
            .chunks_exact(4)
            .all(|p| p[..3] == [255, 255, 255]));
    }
}
