/// Blend eight interleaved RGBA pixels at a time. The ARM build targets Cortex-A7.
pub(crate) unsafe fn blend_solid_span(pixels: &mut [u8], color: [u8; 4]) {
    let alpha = u32::from(color[3]);
    if alpha == 0 {
        return;
    }
    let vector_bytes = pixels.len() / 32 * 32;
    if vector_bytes != 0 {
        // For 0 <= v <= 65025, (v + 1 + (v >> 8)) >> 8 is exactly v / 255.
        core::arch::asm!(
            "vdup.16 q6, {inverse}",
            "vdup.16 q7, {red}",
            "vdup.16 q8, {green}",
            "vdup.16 q9, {blue}",
            "vdup.16 q10, {alpha}",
            "vmov.i16 q12, #1",
            "2:",
            "vld4.8 {{d0, d1, d2, d3}}, [{dest}]",
            "vmovl.u8 q2, d0",
            "vmovl.u8 q3, d1",
            "vmovl.u8 q4, d2",
            "vmovl.u8 q5, d3",
            "vmul.i16 q2, q2, q6",
            "vmul.i16 q3, q3, q6",
            "vmul.i16 q4, q4, q6",
            "vmul.i16 q5, q5, q6",
            "vadd.i16 q2, q2, q7",
            "vadd.i16 q3, q3, q8",
            "vadd.i16 q4, q4, q9",
            "vadd.i16 q5, q5, q10",
            "vshr.u16 q11, q2, #8",
            "vadd.i16 q2, q2, q11",
            "vadd.i16 q2, q2, q12",
            "vshrn.u16 d0, q2, #8",
            "vshr.u16 q11, q3, #8",
            "vadd.i16 q3, q3, q11",
            "vadd.i16 q3, q3, q12",
            "vshrn.u16 d1, q3, #8",
            "vshr.u16 q11, q4, #8",
            "vadd.i16 q4, q4, q11",
            "vadd.i16 q4, q4, q12",
            "vshrn.u16 d2, q4, #8",
            "vshr.u16 q11, q5, #8",
            "vadd.i16 q5, q5, q11",
            "vadd.i16 q5, q5, q12",
            "vshrn.u16 d3, q5, #8",
            "vst4.8 {{d0, d1, d2, d3}}, [{dest}]!",
            "subs {blocks}, {blocks}, #1",
            "bne 2b",
            dest = inout(reg) pixels.as_mut_ptr() => _,
            blocks = inout(reg) vector_bytes / 32 => _,
            inverse = in(reg) 255 - alpha,
            red = in(reg) u32::from(color[0]) * alpha,
            green = in(reg) u32::from(color[1]) * alpha,
            blue = in(reg) u32::from(color[2]) * alpha,
            alpha = in(reg) alpha * 255,
            out("q0") _, out("q1") _, out("q2") _, out("q3") _,
            out("q4") _, out("q5") _, out("q6") _, out("q7") _,
            out("q8") _, out("q9") _, out("q10") _, out("q11") _, out("q12") _,
            options(nostack),
        );
    }
    for pixel in pixels[vector_bytes..].chunks_exact_mut(4) {
        crate::pixel_buffer::blend_source_over_pixel(pixel, color);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn neon_matches_scalar_for_every_alpha_and_unaligned_span_lengths() {
        for alpha in 0..=255 {
            let color = [213, 57, 149, alpha];
            for length in 0..=65 {
                let mut actual: Vec<u8> =
                    (0..length * 4 + 3).map(|n| (n * 37 + 13) as u8).collect();
                let mut expected = actual.clone();
                for pixel in expected[3..].chunks_exact_mut(4) {
                    crate::pixel_buffer::blend_source_over_pixel(pixel, color);
                }
                unsafe {
                    super::blend_solid_span(&mut actual[3..], color);
                }
                assert_eq!(actual, expected, "alpha={alpha} length={length}");
            }
        }
    }
}
