#include <arm_neon.h>
#include <stdint.h>

void balatro_copy_pixels(const uint8_t *restrict source, uint8_t *restrict destination,
                         uint32_t count, uint32_t swap_rb, uint32_t reverse) {
    if (reverse) source += count * 4;
    while (count >= 8) {
        if (reverse) source -= 32;
        uint8x8x4_t pixels = vld4_u8(source);
        if (reverse) {
            for (unsigned i = 0; i < 4; ++i) pixels.val[i] = vrev64_u8(pixels.val[i]);
        }
        if (swap_rb) {
            uint8x8_t red = pixels.val[0];
            pixels.val[0] = pixels.val[2];
            pixels.val[2] = red;
        }
        vst4_u8(destination, pixels);
        if (!reverse) source += 32;
        destination += 32;
        count -= 8;
    }
    while (count--) {
        if (reverse) source -= 4;
        destination[0] = source[swap_rb ? 2 : 0];
        destination[1] = source[1];
        destination[2] = source[swap_rb ? 0 : 2];
        destination[3] = source[3];
        if (!reverse) source += 4;
        destination += 4;
    }
}
