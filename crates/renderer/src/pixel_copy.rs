/// Copy packed RGBA pixels, optionally swapping red/blue or reversing pixel order.
pub fn copy_rgba(source: &[u8], destination: &mut [u8], swap_rb: bool, reverse: bool) {
    assert_eq!(source.len(), destination.len());
    assert_eq!(source.len() % 4, 0);
    if source.is_empty() {
        return;
    }

    #[cfg(all(target_arch = "arm", feature = "arm-neon"))]
    {
        extern "C" {
            fn balatro_copy_pixels(
                source: *const u8,
                destination: *mut u8,
                count: u32,
                swap_rb: u32,
                reverse: u32,
            );
        }
        // Equal, disjoint slices contain complete pixels. The native loop
        // accepts unaligned addresses and handles its final pixels separately.
        unsafe {
            balatro_copy_pixels(
                source.as_ptr(),
                destination.as_mut_ptr(),
                (source.len() / 4) as u32,
                u32::from(swap_rb),
                u32::from(reverse),
            );
        }
        return;
    }

    #[cfg(not(all(target_arch = "arm", feature = "arm-neon")))]
    for (index, target) in destination.chunks_exact_mut(4).enumerate() {
        let offset = if reverse {
            source.len() - 4 * (index + 1)
        } else {
            4 * index
        };
        let pixel = &source[offset..offset + 4];
        target.copy_from_slice(pixel);
        if swap_rb {
            target.swap(0, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_channels_rotation_and_unaligned_tails() {
        let source: Vec<u8> = (0..640 * 4 + 32)
            .map(|i| ((i * 37 + i / 4 * 13) % 256) as u8)
            .collect();
        for count in (0..=33).chain([71, 639, 640]) {
            for source_offset in 0..16 {
                for destination_offset in 0..16 {
                    for swap in [false, true] {
                        for reverse in [false, true] {
                            let input = &source[source_offset..source_offset + count * 4];
                            let mut actual = vec![0x99; destination_offset + count * 4 + 17];
                            let mut expected = actual.clone();
                            let channels = if swap { [2, 1, 0, 3] } else { [0, 1, 2, 3] };
                            for x in 0..count {
                                let source_x = if reverse { count - 1 - x } else { x };
                                for channel in 0..4 {
                                    expected[destination_offset + x * 4 + channel] =
                                        input[source_x * 4 + channels[channel]];
                                }
                            }
                            copy_rgba(
                                input,
                                &mut actual[destination_offset..destination_offset + count * 4],
                                swap,
                                reverse,
                            );
                            assert_eq!(actual, expected,
                                "count={count} source={source_offset} destination={destination_offset} swap={swap} reverse={reverse}");
                        }
                    }
                }
            }
        }
    }
}
