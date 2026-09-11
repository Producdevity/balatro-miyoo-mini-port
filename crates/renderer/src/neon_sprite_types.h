#ifndef BALATRO_SPRITE_TYPES_H
#define BALATRO_SPRITE_TYPES_H

#include <stdint.h>

struct Sprite {
    uint32_t source_width, source_height, target_width;
    int32_t bounds[4];
    float region[4], inverse[6];
    uint32_t tint, white_mask, premultiplied, replace, alpha_shortcuts, row_spans;
    uint32_t packed_pixels;
};

#endif
