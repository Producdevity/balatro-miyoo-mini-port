#include <arm_neon.h>
#include <stdint.h>
#include <string.h>

#include "neon_sprite_types.h"
#include "neon_rgba.h"

#include "neon_sprite_span.h"

typedef uint32x4_t U;
typedef float32x4_t F;
#define U4(x) vdupq_n_u32(x)
#define F4(x) vdupq_n_f32(x)

static inline U div255(U x) {
    return vshrq_n_u32(vmulq_n_u32(x, 32897), 23);
}

static inline uint32_t any(U mask) {
    uint32x2_t halves = vorr_u32(vget_low_u32(mask), vget_high_u32(mask));
    return vget_lane_u32(halves, 0) | vget_lane_u32(halves, 1);
}

static inline uint32_t all(U mask) {
    uint32x2_t halves = vand_u32(vget_low_u32(mask), vget_high_u32(mask));
    return vget_lane_u32(halves, 0) & vget_lane_u32(halves, 1);
}

static inline U channel(U src, U dst, U alpha, U inverse, uint32_t premultiplied) {
    if (premultiplied) return vminq_u32(src + div255(dst * inverse), U4(255));
    return div255(src * alpha + dst * inverse);
}

// Rust validates the image lengths and clipped destination rectangle. Source
// samples are gathered only after both coordinate and integer bounds checks.
static inline __attribute__((always_inline)) void
draw_sprite(const struct Sprite *p, const uint8_t *source, uint8_t *target, int row_spans,
            int packed_pixels, uint32_t origin_x, uint32_t origin_y) {
    const U byte = U4(255);
    const F lane = {0.5f, 1.5f, 2.5f, 3.5f};
    const float *m = p->inverse, *r = p->region;
    const uint32_t white = p->tint == UINT32_MAX;
    const U tr = U4(p->tint & 255), tg = U4((p->tint >> 8) & 255);
    const U tb = U4((p->tint >> 16) & 255), ta = U4(p->tint >> 24);
    const struct Axis u_axis = {m[0], r[2], m[0] == 0 ? 0 : 1.0 / (double)m[0]};
    const struct Axis v_axis = {m[3], r[3], m[3] == 0 ? 0 : 1.0 / (double)m[3]};
    for (int32_t y = p->bounds[1]; y < p->bounds[3]; ++y) {
        float yf = (float)y + 0.5f;
        float bu = m[1] * yf + m[2], bv = m[4] * yf + m[5];
        F base_u = F4(bu), base_v = F4(bv);
        int32_t first = p->bounds[0], last = p->bounds[2];
        if (row_spans) {
            clip_axis(&u_axis, bu, &first, &last);
            clip_axis(&v_axis, bv, &first, &last);
            // Source bounds can round differently after adding the atlas
            // offset. Trim those endpoints before any unchecked gather.
            while (first < last && !source_contains(p, first, bu, bv)) ++first;
            while (first < last && !source_contains(p, last - 1, bu, bv)) --last;
        }
        for (int32_t x = first; x < last; x += 4) {
            uint32_t count = (uint32_t)(last - x);
            if (count > 4) count = 4;
            F xf = F4((float)x) + lane;
            F u = F4(m[0]) * xf + base_u, v = F4(m[3]) * xf + base_v;
            U active = U4(UINT32_MAX);
            U col = vcvtq_u32_f32(F4(r[0]) + u), row = vcvtq_u32_f32(F4(r[1]) + v);
            col -= U4(origin_x);
            row -= U4(origin_y);
            if (!row_spans) {
                active = vcgeq_f32(u, F4(0)) & vcgeq_f32(v, F4(0)) &
                         vcltq_f32(u, F4(r[2])) & vcltq_f32(v, F4(r[3]));
                active &= vcltq_u32(col, U4(p->source_width)) & vcltq_u32(row, U4(p->source_height));
            }
            U offsets = row * U4(p->source_width) + col;
            uint32_t indices[4], mask[4], samples[4] = {0}, background[4] = {0};
            vst1q_u32(indices, offsets);
            vst1q_u32(mask, active);
            uint8_t *out = target + ((uint32_t)y * p->target_width + (uint32_t)x) * 4;
            if (row_spans && count == 4) {
                for (uint32_t n = 0; n < 4; ++n)
                    memcpy(&samples[n], source + indices[n] * 4, 4);
            } else {
                for (uint32_t n = 0; n < count; ++n)
                    if (row_spans || mask[n]) memcpy(&samples[n], source + indices[n] * 4, 4);
            }
            U packed = vld1q_u32(samples);
            U alpha = vshrq_n_u32(packed, 24);
            active &= vcgtq_u32(alpha, U4(0));
            if (p->alpha_shortcuts && !any(active)) continue;
            if (packed_pixels) {
                if (!white) packed = rgba_tint(packed, p->tint, p->white_mask);
                alpha = vshrq_n_u32(packed, 24);
                if (p->alpha_shortcuts && count == 4 &&
                    all(p->replace ? active : vceqq_u32(alpha, byte))) {
                    vst1q_u8(out, vreinterpretq_u8_u32(packed));
                    continue;
                }
                U dst;
                if (count == 4) dst = vreinterpretq_u32_u8(vld1q_u8(out));
                else {
                    memcpy(background, out, count * 4);
                    dst = vld1q_u32(background);
                }
                if (!p->replace) {
                    active &= vcgtq_u32(alpha, U4(0));
                    packed = rgba_over(packed, dst, p->premultiplied);
                }
                packed = vbslq_u32(active, packed, dst);
                if (count == 4) vst1q_u8(out, vreinterpretq_u8_u32(packed));
                else {
                    vst1q_u32(background, packed);
                    memcpy(out, background, count * 4);
                }
                continue;
            }
            U red = packed & byte, green = vshrq_n_u32(packed, 8) & byte;
            U blue = vshrq_n_u32(packed, 16) & byte;
            if (!white) {
                red = p->white_mask ? tr : div255(red * tr);
                green = p->white_mask ? tg : div255(green * tg);
                blue = p->white_mask ? tb : div255(blue * tb);
                alpha = div255(alpha * ta);
            }
            if (p->alpha_shortcuts && count == 4 &&
                all(p->replace ? active : vceqq_u32(alpha, byte))) {
                packed = red | vshlq_n_u32(green, 8) | vshlq_n_u32(blue, 16) | vshlq_n_u32(alpha, 24);
                vst1q_u8(out, vreinterpretq_u8_u32(packed));
                continue;
            }
            U dst;
            if (count == 4) {
                dst = vreinterpretq_u32_u8(vld1q_u8(out));
            } else {
                memcpy(background, out, count * 4);
                dst = vld1q_u32(background);
            }
            if (!p->replace) {
                active &= vcgtq_u32(alpha, U4(0));
                U inverse = byte - alpha;
                red = channel(red, dst & byte, alpha, inverse, p->premultiplied);
                green = channel(green, vshrq_n_u32(dst, 8) & byte, alpha, inverse, p->premultiplied);
                blue = channel(blue, vshrq_n_u32(dst, 16) & byte, alpha, inverse, p->premultiplied);
                alpha += div255(vshrq_n_u32(dst, 24) * inverse);
            }
            packed = red | vshlq_n_u32(green, 8) | vshlq_n_u32(blue, 16) | vshlq_n_u32(alpha, 24);
            packed = vbslq_u32(active, packed, dst);
            if (count == 4) {
                vst1q_u8(out, vreinterpretq_u8_u32(packed));
            } else {
                vst1q_u32(background, packed);
                memcpy(out, background, count * 4);
            }
        }
    }
}

void balatro_nearest_sprite(const struct Sprite *p, const uint8_t *source, uint8_t *target) {
    if (p->packed_pixels) {
        if (p->row_spans) draw_sprite(p, source, target, 1, 1, 0, 0);
        else draw_sprite(p, source, target, 0, 1, 0, 0);
    } else {
        if (p->row_spans) draw_sprite(p, source, target, 1, 0, 0, 0);
        else draw_sprite(p, source, target, 0, 0, 0, 0);
    }
}

void balatro_prepared_sprite(const struct Sprite *p, const uint8_t *source, uint8_t *target,
                             uint32_t origin_x, uint32_t origin_y) {
    // Keep atlas rounding, then translate into the cropped prepared image.
    if (p->packed_pixels) draw_sprite(p, source, target, 0, 1, origin_x, origin_y);
    else draw_sprite(p, source, target, 0, 0, origin_x, origin_y);
}
