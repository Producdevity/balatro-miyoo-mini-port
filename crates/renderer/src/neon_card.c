#include "neon_card_kernel.h"

void balatro_card_shader(uint32_t effect, const struct Shader *sp, struct Batch *batch,
                         size_t count) {
    // Specialize the loop so each effect keeps only its own constants and branches.
    switch (effect) {
    case 1:
        shade_effect(1, sp, batch, count, NULL);
        break;
    case 2:
        shade_effect(2, sp, batch, count, NULL);
        break;
    case 3:
        shade_effect(3, sp, batch, count, NULL);
        break;
    case 4:
        shade_effect(4, sp, batch, count, NULL);
        break;
    case 5:
        shade_effect(5, sp, batch, count, NULL);
        break;
    case 6:
        shade_effect(6, sp, batch, count, NULL);
        break;
    }
}
