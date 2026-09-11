#ifndef BALATRO_CARD_KERNEL_H
#define BALATRO_CARD_KERNEL_H

#include <arm_neon.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>

typedef float32x4_t F;
#define S(x) vdupq_n_f32(x)

struct Shader {
    float texture[2], phase, clock, foil[6], offsets[6];
    const float *sine;
    const float *byte_fractions;
    uint32_t noise_axes;
};

struct NoiseAxes {
    float x[256], y[256];
    uint32_t columns, rows;
};

struct Batch {
    uint32_t rgba[32];
    float u[32], v[32], h[32], s[32], l[32];
};

static inline F low(F a, F b) { return vminq_f32(a, b); }
static inline F high(F a, F b) { return vmaxq_f32(a, b); }
static inline F clamp(F x) { return low(high(x, S(0)), S(1)); }
static inline F select(uint32x4_t mask, F yes, F no) { return vbslq_f32(mask, yes, no); }

static inline F normalized(const struct Shader *sp, uint32x4_t bytes) {
    const float *lut = sp->byte_fractions;
    if (!lut) return vcvtq_f32_u32(bytes) / S(255);
    float values[4] = {lut[vgetq_lane_u32(bytes, 0)], lut[vgetq_lane_u32(bytes, 1)],
                       lut[vgetq_lane_u32(bytes, 2)], lut[vgetq_lane_u32(bytes, 3)]};
    return vld1q_f32(values);
}

static inline F faded_alpha(const struct Shader *sp, uint32x4_t alpha) {
    if (!sp->byte_fractions) {
        return vcvtq_f32_u32(vcvtq_u32_f32(vcvtq_f32_u32(alpha) / S(3)));
    }
    // floor(a / 3), exact for every byte value; no floating-point division.
    return vcvtq_f32_u32(vshrq_n_u32(vmulq_n_u32(alpha, 171), 9));
}

static inline F root(F v) {
    float r[4] = {sqrtf(vgetq_lane_f32(v, 0)), sqrtf(vgetq_lane_f32(v, 1)),
                  sqrtf(vgetq_lane_f32(v, 2)), sqrtf(vgetq_lane_f32(v, 3))};
    return vld1q_f32(r);
}

static inline F sine(const struct Shader *sp, F x) {
    F phase = x * S(4096.0f / 6.28318530717958647692f);
    F rounded = phase + select(vcgeq_f32(phase, S(0)), S(0.5f), S(-0.5f));
    int32x4_t index = vcvtq_s32_f32(rounded);
    const float *lut = sp->sine;
    float values[4] = {lut[(uint32_t)vgetq_lane_s32(index, 0) & 4095],
                       lut[(uint32_t)vgetq_lane_s32(index, 1) & 4095],
                       lut[(uint32_t)vgetq_lane_s32(index, 2) & 4095],
                       lut[(uint32_t)vgetq_lane_s32(index, 3) & 4095]};
    return vld1q_f32(values);
}

static inline F cosine(const struct Shader *sp, F x) {
    return sine(sp, x + S(1.57079632679489661923f));
}

static inline F wrap(F x) {
    F magnitude = vabsq_f32(x);
    F fraction = select(vcltq_f32(magnitude, S(1)), x, x - vcvtq_f32_s32(vcvtq_s32_f32(x)));
    F shifted = fraction + S(1);
    return select(vcgeq_f32(shifted, S(2)), S(0),
                  select(vcgeq_f32(shifted, S(1)), shifted - S(1), shifted));
}

static inline F hue(F h, F t, F sv) {
    F x = wrap(h) * S(6);
    return select(
        vcltq_f32(x, S(1)), (t - sv) * x + sv,
        select(vcltq_f32(x, S(3)), t, select(vcltq_f32(x, S(4)), (t - sv) * (S(4) - x) + sv, sv)));
}

static inline void hsl(F h, F s, F l, F *r, F *g, F *b) {
    F t = select(vcltq_f32(l, S(0.5f)), s * l + l, -s * l + (s + l));
    F sv = S(2) * l - t;
    uint32x4_t grey = vcltq_f32(s, S(0.0001f));
    *r = select(grey, l, hue(h + S(1.0f / 3.0f), t, sv));
    *g = select(grey, l, hue(h, t, sv));
    *b = select(grey, l, hue(h - S(1.0f / 3.0f), t, sv));
}

static inline F foil(const struct Shader *sp, F u, F v) {
    F phase = S(sp->phase), clock = S(sp->clock);
    F ax = (u - S(0.5f)) * S(sp->texture[0] / sp->texture[1]);
    F ay = v - S(0.5f);
    F length = root(ax * ax + ay * ay), len90 = length * S(90);
    F a = clamp(S(2) * sine(sp, len90 + phase * S(2) +
                                    S(3) * (S(1) + S(0.8f) * cosine(sp, length * S(113.1121f) -
                                                                            phase * S(3.121f)))) -
                S(1) - high(S(5) - len90, S(0)));
    F angle =
        (S(sp->foil[0]) * ax + S(sp->foil[1]) * ay) / (S(sp->foil[2]) * high(length, S(0.001f)));
    F b = clamp(
        S(5) * cosine(sp, clock * S(0.3f) + angle * S(3.14f) * (S(2.2f) + S(0.9f * sp->foil[3]))) -
        S(4) - high(S(2) - length * S(20), S(0)));
    F c =
        S(0.3f) *
        low(high(S(2) * sine(sp, phase * S(5) + u * S(3) + S(3 * (1 + 0.5f * sp->foil[4]))) - S(1),
                 S(-1)),
            S(1));
    F d =
        S(0.3f) *
        low(high(S(2) * sine(sp, phase * S(6.66f) + v * S(3.8f) + S(3 * (1 + 0.5f * sp->foil[5]))) -
                     S(1),
                 S(-1)),
            S(1));
    return high(high(a, high(b, high(c, high(d, S(0))))) + S(2.2f) * (a + b + c + d), S(0));
}

static inline F noise_axis(uint32x4_t indices, const float *values,
                          uint32_t count, float size, float scale) {
    uint32_t a = vgetq_lane_u32(indices, 0), b = vgetq_lane_u32(indices, 1);
    uint32_t c = vgetq_lane_u32(indices, 2), d = vgetq_lane_u32(indices, 3);
    if (values && a < count && b < count && c < count && d < count) {
        float samples[4] = {values[a], values[b], values[c], values[d]};
        return vld1q_f32(samples);
    }
    return (vcvtq_f32_u32(indices) / S(size) - S(0.5f)) * S(scale);
}

static inline F noise(const struct Shader *sp, F u, F v, float scale,
                      const struct NoiseAxes *axes) {
    uint32x4_t column = vcvtq_u32_f32(u * S(sp->texture[0]));
    uint32x4_t row = vcvtq_u32_f32(v * S(sp->texture[1]));
    F x = noise_axis(column, axes ? axes->x : NULL, axes ? axes->columns : 0,
                     sp->texture[0], scale);
    F y = noise_axis(row, axes ? axes->y : NULL, axes ? axes->rows : 0,
                     sp->texture[1], scale);
    F ax = x + S(sp->offsets[0]), ay = y + S(sp->offsets[1]);
    F bx = x + S(sp->offsets[2]), by = y + S(sp->offsets[3]);
    F cx = x + S(sp->offsets[4]), cy = y + S(sp->offsets[5]);
    F field = (S(1) + cosine(sp, root(ax * ax + ay * ay) / S(19.483f)) +
               sine(sp, root(bx * bx + by * by) / S(33.155f)) * cosine(sp, by / S(15.73f)) +
               cosine(sp, root(cx * cx + cy * cy) / S(27.193f)) * sine(sp, cx / S(21.92f))) /
              S(2);
    return S(0.5f) + S(0.5f) * cosine(sp, S(sp->phase * 2.612f) + (field - S(0.5f)) * S(3.14f));
}

// Each batch contains only visible source samples from one draw, in draw order.
static inline __attribute__((always_inline)) void
shade_effect(uint32_t effect, const struct Shader *sp, struct Batch *batch, size_t count,
             const struct NoiseAxes *axes) {
    const uint32x4_t byte = vdupq_n_u32(255);
    for (size_t i = 0; i < count; i += 4) {
        uint32x4_t packed = vld1q_u32(batch->rgba + i);
        F r = normalized(sp, vandq_u32(packed, byte));
        F g = normalized(sp, vandq_u32(vshrq_n_u32(packed, 8), byte));
        F b = normalized(sp, vandq_u32(vshrq_n_u32(packed, 16), byte));
        uint32x4_t alpha_bytes = vshrq_n_u32(packed, 24);
        F alpha = vcvtq_f32_u32(alpha_bytes);
        F u = vld1q_f32(batch->u + i), v = vld1q_f32(batch->v + i);
        if (effect == 3) {
            F fac = foil(sp, u, v);
            F delta = low(high(r, high(g, b)), high(S(1) - low(r, low(g, b)), S(0.5f)));
            r = clamp(r - delta + delta * fac * S(0.3f));
            g = clamp(g - delta + delta * fac * S(0.3f));
            b = clamp(b + delta * fac * S(1.9f));
            F a = normalized(sp, alpha_bytes);
            alpha = low(a, S(0.3f) * a + S(0.9f) * low(fac * S(0.1f), S(0.5f))) * S(255);
        } else if (effect == 1 || effect == 2 || effect == 6) {
            F h = vld1q_f32(batch->h + i), s = vld1q_f32(batch->s + i), l = vld1q_f32(batch->l + i);
            if (effect == 1) {
                hsl(h, s * S(0.5f), l * S(0.8f), &r, &g, &b);
                alpha = vcvtq_f32_u32(vshrq_n_u32(vcvtq_u32_f32(alpha), 1));
            } else if (effect == 2) {
                uint32x4_t stripe = vorrq_u32(vcltq_f32(vabsq_f32(u + v - S(1)), S(0.1f)),
                                              vcltq_f32(vabsq_f32((S(1) - u) + v - S(1)), S(0.1f)));
                hsl(select(stripe, S(1), h), select(stripe, S(0.7f), S(0.25f)),
                    select(stripe, l * S(0.8f), l * S(0.7f)), &r, &g, &b);
                F faded = vcvtq_f32_u32(vshrq_n_u32(vmulq_n_u32(vcvtq_u32_f32(alpha), 77), 8));
                alpha = select(stripe, alpha, faded);
            } else {
                hsl(wrap(-h + S(0.2f)), s, sp->clock != 0 ? S(1) - l : l, &r, &g, &b);
                r += S(0.8f * 79.0f / 255.0f);
                g += S(0.8f * 99.0f / 255.0f);
                b += S(0.8f * 103.0f / 255.0f);
                alpha = select(vcltq_f32(alpha, S(178.5f)), faded_alpha(sp, alpha_bytes), alpha);
            }
        } else {
            F h = vld1q_f32(batch->h + i), s = vld1q_f32(batch->s + i), l = vld1q_f32(batch->l + i);
            F res = noise(sp, u, v, effect == 4 ? 250 : 50, axes);
            if (effect == 4) {
                F grid1 = high(S(7) * vabsq_f32(cosine(sp, u * S(0.79f) * S(20))) - S(6), S(0));
                F grid2 = high(
                    S(7) * cosine(sp, v * S(0.79f) * S(45) + u * S(0.79f) * S(20)) - S(6), S(0));
                F grid3 = high(
                    S(7) * cosine(sp, v * S(0.79f) * S(45) - u * S(0.79f) * S(20)) - S(6), S(0));
                F fac = S(0.5f) * high(grid1, high(grid2, grid3));
                F hi = high(r, high(g, b)), lo = low(r, low(g, b));
                F delta = S(0.2f) + S(0.3f) * (hi - lo) + S(0.1f) * hi;
                F hr, hg, hb;
                hsl(wrap(h + res + fac), low(s * S(1.3f), S(1)), low(l * S(0.6f) + S(0.4f), S(1)),
                    &hr, &hg, &hb);
                r = (S(1) - delta) * r + delta * hr * S(0.9f);
                g = (S(1) - delta) * g + delta * hg * S(0.8f);
                b = (S(1) - delta) * b + delta * hb * S(1.2f);
            } else {
                F saturation = low(high(low(s, S(0.6f)), s + S(0.5f)), S(0.6f));
                hsl(wrap(h + res + S(sp->clock * 0.04f)), saturation, l, &r, &g, &b);
            }
            alpha = select(vcltq_f32(alpha, S(178.5f)), faded_alpha(sp, alpha_bytes), alpha);
        }
        uint32x4_t result = vcvtq_u32_f32(clamp(r) * S(255));
        result = vorrq_u32(result, vshlq_n_u32(vcvtq_u32_f32(clamp(g) * S(255)), 8));
        result = vorrq_u32(result, vshlq_n_u32(vcvtq_u32_f32(clamp(b) * S(255)), 16));
        result = vorrq_u32(result, vshlq_n_u32(vcvtq_u32_f32(alpha), 24));
        vst1q_u32(batch->rgba + i, result);
    }
}

#endif
