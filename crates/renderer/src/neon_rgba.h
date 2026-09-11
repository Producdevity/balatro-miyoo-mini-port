#ifndef BALATRO_NEON_RGBA_H
#define BALATRO_NEON_RGBA_H

#include <arm_neon.h>

// Exact floor(x / 255) for products and weighted sums in [0, 65025].
static inline uint8x8_t rgba_div255(uint16x8_t x) {
    return vshrn_n_u16(vaddq_u16(vaddq_u16(x, vdupq_n_u16(1)), vshrq_n_u16(x, 8)), 8);
}

static inline uint32x4_t rgba_tint(uint32x4_t source, uint32_t tint, uint32_t white_mask) {
    if (white_mask) source |= vdupq_n_u32(0x00ffffff);
    uint8x16_t src = vreinterpretq_u8_u32(source);
    uint8x16_t color = vreinterpretq_u8_u32(vdupq_n_u32(tint));
    uint8x8_t lo = rgba_div255(vmull_u8(vget_low_u8(src), vget_low_u8(color)));
    uint8x8_t hi = rgba_div255(vmull_u8(vget_high_u8(src), vget_high_u8(color)));
    return vreinterpretq_u32_u8(vcombine_u8(lo, hi));
}

static inline uint32x4_t rgba_over(uint32x4_t source, uint32x4_t destination,
                                  uint32_t premultiplied) {
    uint32x4_t alpha = vmulq_n_u32(vshrq_n_u32(source, 24), 0x01010101);
    uint8x16_t a = vreinterpretq_u8_u32(alpha);
    uint8x16_t inverse = vsubq_u8(vdupq_n_u8(255), a);
    uint8x16_t dst = vreinterpretq_u8_u32(destination);
    uint16x8_t lo = vmull_u8(vget_low_u8(dst), vget_low_u8(inverse));
    uint16x8_t hi = vmull_u8(vget_high_u8(dst), vget_high_u8(inverse));
    if (premultiplied) {
        uint8x16_t faded = vcombine_u8(rgba_div255(lo), rgba_div255(hi));
        return vreinterpretq_u32_u8(vqaddq_u8(vreinterpretq_u8_u32(source), faded));
    }
    // Alpha uses a + dst.a * (1-a), not a*a + dst.a * (1-a).
    uint8x16_t src = vreinterpretq_u8_u32(source | vdupq_n_u32(0xff000000));
    lo = vmlal_u8(lo, vget_low_u8(src), vget_low_u8(a));
    hi = vmlal_u8(hi, vget_high_u8(src), vget_high_u8(a));
    return vreinterpretq_u32_u8(vcombine_u8(rgba_div255(lo), rgba_div255(hi)));
}

#endif
