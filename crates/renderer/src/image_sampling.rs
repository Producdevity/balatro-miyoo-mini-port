// Alpha-weighted box filtering. White font masks need only the alpha average.
#[inline(always)]
pub(crate) fn average_box<const WHITE: bool>(
    pixels: &[u8],
    width: u32,
    bounds: [u32; 4],
    step: [u32; 2],
) -> Option<[u8; 4]> {
    let [x0, y0, x1, y1] = bounds;
    let [step_x, step_y] = step;
    let mut sums = [0_u32; 4];
    let mut count = 0;
    let mut y = y0;
    while y < y1 {
        let row = (y * width) as usize * 4;
        let mut x = x0;
        while x < x1 {
            let index = row + x as usize * 4;
            if index + 3 >= pixels.len() {
                break;
            }
            let alpha = u32::from(pixels[index + 3]);
            if !WHITE {
                sums[0] += u32::from(pixels[index]) * alpha;
                sums[1] += u32::from(pixels[index + 1]) * alpha;
                sums[2] += u32::from(pixels[index + 2]) * alpha;
            }
            sums[3] += alpha;
            count += 1;
            x += step_x;
        }
        y += step_y;
    }
    if count == 0 {
        return None;
    }
    let alpha = sums[3];
    if alpha == 0 {
        return Some([0; 4]);
    }
    Some(if WHITE {
        [255, 255, 255, (alpha / count) as u8]
    } else {
        [
            (sums[0] / alpha) as u8,
            (sums[1] / alpha) as u8,
            (sums[2] / alpha) as u8,
            (alpha / count) as u8,
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_mask_matches_full_colour_average_at_edges_and_sampling_steps() {
        let source: Vec<u8> = (0..71 * 95)
            .flat_map(|i| {
                let alpha = ((i * 37) % 256) as u8;
                if alpha == 0 {
                    [11, 31, 97, 0]
                } else {
                    [255, 255, 255, alpha]
                }
            })
            .collect();
        for x in 0..71 {
            for y in 0..95 {
                for step in [[1, 1], [2, 1], [1, 3], [3, 4]] {
                    let bounds = [x, y, (x + 9).min(71), (y + 7).min(95)];
                    assert_eq!(
                        average_box::<true>(&source, 71, bounds, step),
                        average_box::<false>(&source, 71, bounds, step)
                    );
                }
            }
        }
        for source in [&[][..], &[97, 17, 255, 0][..]] {
            assert_eq!(
                average_box::<true>(source, 1, [0, 0, 1, 1], [1, 1]),
                average_box::<false>(source, 1, [0, 0, 1, 1], [1, 1])
            );
        }
    }
}
