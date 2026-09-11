#include "neon_card_kernel.h"
#include "neon_sprite_types.h"
#include "neon_sprite_span.h"
#include <string.h>

typedef void (*FillHsl)(void *, uint32_t, struct Batch *, size_t);

static inline uint32_t blend_channel(uint32_t src, uint32_t dst, uint32_t alpha,
                                     uint32_t inverse, uint32_t premultiplied) {
    if (!premultiplied) return (src * alpha + dst * inverse) / 255;
    uint32_t value = src + dst * inverse / 255;
    return value < 255 ? value : 255;
}

static inline __attribute__((always_inline)) void
finish_batch(uint32_t effect, const struct Sprite *p, const struct Shader *shader,
             struct Batch *batch, const uint32_t *destinations, size_t count,
             uint8_t *target, void *cache, FillHsl fill_hsl, const struct NoiseAxes *axes) {
    if (!count) return;
    if (effect != 3) fill_hsl(cache, effect, batch, count);
    shade_effect(effect, shader, batch, (count + 3) & ~(size_t)3, axes);
    for (size_t i = 0; i < count; ++i) {
        uint32_t src = batch->rgba[i], alpha = src >> 24;
        uint8_t *out = target + destinations[i];
        if (p->replace || alpha == 255) {
            memcpy(out, &src, 4);
        } else if (alpha) {
            uint32_t dst;
            memcpy(&dst, out, 4);
            uint32_t inverse = 255 - alpha;
            uint32_t red = blend_channel(src & 255, dst & 255, alpha, inverse, p->premultiplied);
            uint32_t green = blend_channel((src >> 8) & 255, (dst >> 8) & 255,
                                           alpha, inverse, p->premultiplied);
            uint32_t blue = blend_channel((src >> 16) & 255, (dst >> 16) & 255,
                                          alpha, inverse, p->premultiplied);
            alpha += (dst >> 24) * inverse / 255;
            uint32_t result = red | (green << 8) | (blue << 16) | (alpha << 24);
            memcpy(out, &result, 4);
        }
    }
}

// Only coordinates in the clipped row reach the source gather. The small
// batch keeps the existing colour cache and shader math, without a frame copy.
static inline __attribute__((always_inline)) void
draw_shader(uint32_t effect, const struct Sprite *p, const struct Shader *shader,
            const uint8_t *source, uint8_t *target, void *cache, FillHsl fill_hsl,
            const struct Sprite *overlay, const uint8_t *overlay_source) {
    struct Batch batch = {0};
    uint32_t destinations[32];
    size_t used = 0;
    struct NoiseAxes axes;
    const struct NoiseAxes *lookup = NULL;
    if ((effect == 4 || effect == 5) && shader->noise_axes &&
        shader->texture[0] > 0 && shader->texture[0] < 256 &&
        shader->texture[1] > 0 && shader->texture[1] < 256) {
        float scale = effect == 4 ? 250 : 50;
        axes.columns = (uint32_t)shader->texture[0] + 1;
        axes.rows = (uint32_t)shader->texture[1] + 1;
        // The shader quantizes these coordinates before dividing. Precompute
        // the same float operations once per axis, not once per visible pixel.
        for (uint32_t i = 0; i < axes.columns; ++i)
            axes.x[i] = ((float)i / shader->texture[0] - 0.5f) * scale;
        for (uint32_t i = 0; i < axes.rows; ++i)
            axes.y[i] = ((float)i / shader->texture[1] - 0.5f) * scale;
        lookup = &axes;
    }
    const float *m = p->inverse, *r = p->region;
    const F lanes = {0.5f, 1.5f, 2.5f, 3.5f};
    const uint32x4_t byte = vdupq_n_u32(255);
    const struct Axis u_axis = {m[0], r[2], m[0] == 0 ? 0 : 1.0 / (double)m[0]};
    const struct Axis v_axis = {m[3], r[3], m[3] == 0 ? 0 : 1.0 / (double)m[3]};
    for (int32_t y = p->bounds[1]; y < p->bounds[3]; ++y) {
        float yf = (float)y + 0.5f;
        float bu = m[1] * yf + m[2], bv = m[4] * yf + m[5];
        int32_t first = p->bounds[0], last = p->bounds[2];
        clip_axis(&u_axis, bu, &first, &last);
        clip_axis(&v_axis, bv, &first, &last);
        for (int32_t x = first; x < last; x += 4) {
            uint32_t count = (uint32_t)(last - x);
            if (count > 4) count = 4;
            F xf = S((float)x) + lanes;
            F u = S(m[0]) * xf + S(bu), v = S(m[3]) * xf + S(bv);
            uint32x4_t col = vcvtq_u32_f32(S(r[0]) + u);
            uint32x4_t row = vcvtq_u32_f32(S(r[1]) + v);
            uint32x4_t active = vcltq_u32(col, vdupq_n_u32(p->source_width)) &
                                vcltq_u32(row, vdupq_n_u32(p->source_height));
            uint32_t indices[4], mask[4], samples[4] = {0};
            vst1q_u32(indices, row * vdupq_n_u32(p->source_width) + col);
            vst1q_u32(mask, active);
            for (uint32_t n = 0; n < count; ++n)
                if (mask[n]) memcpy(&samples[n], source + indices[n] * 4, 4);
            if (overlay) {
                // Keep each atlas origin in its original floating-point gather.
                // The caller guarantees binary overlay alpha and matching state.
                uint32x4_t top_col = vcvtq_u32_f32(S(overlay->region[0]) + u);
                uint32x4_t top_row = vcvtq_u32_f32(S(overlay->region[1]) + v);
                uint32x4_t top_active =
                    vcltq_u32(top_col, vdupq_n_u32(overlay->source_width)) &
                    vcltq_u32(top_row, vdupq_n_u32(overlay->source_height));
                vst1q_u32(indices, top_row * vdupq_n_u32(overlay->source_width) + top_col);
                vst1q_u32(mask, top_active);
                for (uint32_t n = 0; n < count; ++n) {
                    uint32_t top = 0;
                    if (mask[n]) memcpy(&top, overlay_source + indices[n] * 4, 4);
                    if ((top >> 24) == 255) samples[n] = top;
                }
                active |= top_active;
            }
            uint32x4_t packed = vld1q_u32(samples);
            uint32x4_t alpha = vshrq_n_u32(packed, 24);
            vst1q_u32(mask, active & vcgtq_u32(alpha, vdupq_n_u32(0)));
            if (p->tint != UINT32_MAX) {
                uint32x4_t red = packed & byte, green = vshrq_n_u32(packed, 8) & byte;
                uint32x4_t blue = vshrq_n_u32(packed, 16) & byte;
                uint32x4_t tr = vdupq_n_u32(p->tint & 255);
                uint32x4_t tg = vdupq_n_u32((p->tint >> 8) & 255);
                uint32x4_t tb = vdupq_n_u32((p->tint >> 16) & 255);
                red = p->white_mask ? tr : vshrq_n_u32(vmulq_n_u32(red * tr, 32897), 23);
                green = p->white_mask ? tg : vshrq_n_u32(vmulq_n_u32(green * tg, 32897), 23);
                blue = p->white_mask ? tb : vshrq_n_u32(vmulq_n_u32(blue * tb, 32897), 23);
                alpha = vshrq_n_u32(vmulq_n_u32(alpha * vdupq_n_u32(p->tint >> 24), 32897), 23);
                packed = red | vshlq_n_u32(green, 8) | vshlq_n_u32(blue, 16) |
                         vshlq_n_u32(alpha, 24);
            }
            vst1q_u32(samples, packed);
            float us[4], vs[4];
            vst1q_f32(us, u / S(r[2]));
            vst1q_f32(vs, v / S(r[3]));
            for (uint32_t n = 0; n < count; ++n) {
                if (!mask[n]) continue;
                batch.rgba[used] = samples[n];
                batch.u[used] = us[n];
                batch.v[used] = vs[n];
                destinations[used] = ((uint32_t)y * p->target_width + (uint32_t)x + n) * 4;
                if (++used == 32) {
                    finish_batch(effect, p, shader, &batch, destinations, used, target, cache, fill_hsl, lookup);
                    used = 0;
                }
            }
        }
    }
    finish_batch(effect, p, shader, &batch, destinations, used, target, cache, fill_hsl, lookup);
}

void balatro_shaded_sprite(uint32_t effect, const struct Sprite *p, const struct Shader *shader,
                           const uint8_t *source, uint8_t *target, void *cache, FillHsl fill_hsl) {
    switch (effect) {
    case 1: draw_shader(1, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    case 2: draw_shader(2, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    case 3: draw_shader(3, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    case 4: draw_shader(4, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    case 5: draw_shader(5, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    case 6: draw_shader(6, p, shader, source, target, cache, fill_hsl, NULL, NULL); break;
    }
}

void balatro_shaded_sprite_pair(uint32_t effect, const struct Sprite *base,
                               const struct Sprite *overlay, const struct Shader *shader,
                               const uint8_t *source, const uint8_t *overlay_source,
                               uint8_t *target, void *cache, FillHsl fill_hsl) {
    switch (effect) {
    case 4: draw_shader(4, base, shader, source, target, cache, fill_hsl,
                        overlay, overlay_source); break;
    case 5: draw_shader(5, base, shader, source, target, cache, fill_hsl,
                        overlay, overlay_source); break;
    }
}
