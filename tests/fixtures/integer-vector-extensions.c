typedef int iv4 __attribute__((vector_size(16)));
#if defined(VECTOR_INCREMENT)
void update(iv4 *p) { (*p)++; }
#elif defined(VECTOR_ADDRESS)
int *address(iv4 *p) { return &(*p)[0]; }
#elif defined(ARM_TYPES)
#include <stdio.h>
typedef signed char bytes8 __attribute__((vector_size(8)));
typedef unsigned int words4 __attribute__((vector_size(16)));
int main(void) {
    printf("%lu %lu %d %lu %lu %d\n", sizeof(__Int8x8_t), _Alignof(__Int8x8_t),
        __builtin_types_compatible_p(__Int8x8_t, bytes8),
        sizeof(__Uint32x4_t), _Alignof(__Uint32x4_t),
        __builtin_types_compatible_p(__Uint32x4_t, words4));
    __Int64x2_t s = {1, 2}; __Uint64x2_t u = {1, 2};
    printf("lane64 %d %d %d %d\n",
        __builtin_types_compatible_p(__typeof__(s[0]), long),
        __builtin_types_compatible_p(__typeof__(s[0]), long long),
        __builtin_types_compatible_p(__typeof__(u[0]), unsigned long),
        __builtin_types_compatible_p(__typeof__(u[0]), unsigned long long));
}
#endif
