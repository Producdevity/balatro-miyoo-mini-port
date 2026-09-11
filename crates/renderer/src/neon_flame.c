#include <arm_neon.h>
#include <math.h>

typedef float32x4_t F;
#define V(x) vdupq_n_f32(x)

extern F xsinf(F);
extern F xcosf(F);

static F root(F x) {
    float result[4] = {sqrtf(vgetq_lane_f32(x, 0)), sqrtf(vgetq_lane_f32(x, 1)),
                       sqrtf(vgetq_lane_f32(x, 2)), sqrtf(vgetq_lane_f32(x, 3))};
    return vld1q_f32(result);
}

// Inputs are the same draw constants and quantized coordinates as Flame::density.
// SLEEF supplies vector sine/cosine; contraction stays disabled in this kernel.
void balatro_flame_density4(const float *params, const float *xs, float y, float *output) {
    float time = params[0], intensity = params[1], rise = params[2];
    float scale = params[3], speed = params[4];
    F x = vld1q_f32(xs), vy = V(y);
    F wobble = V(0.01f) * xsinf(V(-1.123f) * x + V(0.2f * time)) *
               xcosf(V(5.3332f) * vy + V(time * 0.931f));
    F ux = x + x * wobble, uy = vy + vy * wobble;
    F sx = ux * V(scale), sy = uy * V(scale) + V(rise), x2 = V(0), y2 = V(0);
    for (int i = 0; i < 5; ++i) {
        F length = root(sx * sx + sy * sy);
        F noise = V(0.3f) * (xcosf(length * V(0.411f)) + V(0.3344f) * xsinf(length) -
                             V(0.23f) * xcosf(length));
        F nx = x2 + sx + V(0.05f) * y2 + noise;
        F ny = y2 + sy + V(0.05f) * x2 + noise;
        x2 = nx;
        y2 = ny;
        sx += V(0.5f) * xcosf(xcosf(y2) + V(speed * 0.0812f)) *
              xsinf(V(3.22f) + x2 - V(speed * 0.1531f));
        sy += V(0.5f) * xsinf(-x2 * V(1.21222f) + V(0.113785f * speed)) *
              xcosf(y2 * V(0.91213f) - V(0.13582f * speed));
    }
    F dx = sx / V(scale) * V(5), dy = (sy - V(rise)) / V(scale) * V(5);
    F smoke = vmaxq_f32(root(dx * dx + dy * dy) + V(0.1f) * (root(ux * ux + uy * uy) - V(0.5f)), V(0)) *
              V(2.0f / (2.0f + intensity * 0.2f));
    F fade = V(fmaxf(2.0f - 0.3f * intensity, 0.0f)) *
             vmaxq_f32(V(2) * (uy - V(0.5f)) * (uy - V(0.5f)), V(0));
    vst1q_f32(output, smoke + fade);
}
