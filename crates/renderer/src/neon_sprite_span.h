struct Axis {
    float slope, limit;
    double reciprocal;
};

static int passed(const struct Axis *axis, int32_t x, float base, float edge) {
    float value = axis->slope * ((float)x + 0.5f) + base;
    return axis->slope > 0 ? value >= edge : value < edge;
}

static int32_t crossing(const struct Axis *axis, int32_t start, int32_t end,
                         float base, float edge) {
    double estimate = ((double)edge - (double)base) * axis->reciprocal - 0.5;
    int32_t x = estimate <= start ? start : estimate >= end ? end : (int32_t)estimate;
    // Correct the estimate using the raster loop's float evaluation, including
    // boundary rounding. There is no change to texture sampling coordinates.
    while (x > start && passed(axis, x - 1, base, edge)) --x;
    while (x < end && !passed(axis, x, base, edge)) ++x;
    return x;
}

static void clip_axis(const struct Axis *axis, float base, int32_t *start, int32_t *end) {
    if (*start >= *end) return;
    if (axis->slope == 0) {
        if (!(base >= 0 && base < axis->limit)) *end = *start;
        return;
    }
    float entry = axis->slope > 0 ? 0 : axis->limit;
    float exit = axis->slope > 0 ? axis->limit : 0;
    *start = crossing(axis, *start, *end, base, entry);
    *end = crossing(axis, *start, *end, base, exit);
}

static inline int source_contains(const struct Sprite *p, int32_t x, float base_u, float base_v) {
    float u = p->region[0] + (p->inverse[0] * ((float)x + 0.5f) + base_u);
    float v = p->region[1] + (p->inverse[3] * ((float)x + 0.5f) + base_v);
    // Match NEON's saturating unsigned conversion for negative atlas offsets.
    uint32_t column = u <= 0 ? 0 : (uint32_t)u;
    uint32_t row = v <= 0 ? 0 : (uint32_t)v;
    return column < p->source_width && row < p->source_height;
}
