#include <arm_neon.h>

_Static_assert(sizeof(float16x4_t) == 8, "half vector storage");
_Static_assert(sizeof(float16x8_t) == 16, "wide half vector storage");
_Static_assert(sizeof(float32x2_t) == 8, "float vector storage");
_Static_assert(sizeof(float64x1_t) == 8, "double vector storage");
_Static_assert(sizeof(bfloat16x4_t) == 8, "bfloat vector storage");
_Static_assert(sizeof(bfloat16x8_t) == 16, "wide bfloat vector storage");
_Static_assert(sizeof(poly8_t) == 1, "polynomial scalar storage");
_Static_assert(sizeof(poly16_t) == 2, "polynomial scalar storage");
_Static_assert(sizeof(poly64_t) == 8, "polynomial scalar storage");
_Static_assert(sizeof(poly128_t) == 16, "polynomial scalar storage");
_Static_assert(sizeof(poly8x16_t) == 16, "polynomial vector storage");
_Static_assert(sizeof(uint8x8x3_t) == 24, "tuple storage");
_Static_assert(__alignof__(uint8x8x3_t) == 8, "tuple alignment");
_Static_assert(sizeof(uint64x2x4_t) == 64, "wide tuple storage");
_Static_assert(__alignof__(uint64x2x4_t) == 16, "wide tuple alignment");
_Static_assert(sizeof(((uint8x8x3_t *)0)->val) == 24, "tuple member");

extern float16x4_t declared_only(float16x4_t);
extern const poly8_t *declared_polynomial(const poly8_t *);

int main(void) { return 0; }
