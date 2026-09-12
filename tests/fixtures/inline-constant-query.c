#define INLINE static inline __attribute__((always_inline))
extern void constant_required(void) __attribute__((error("constant required")));
INLINE int inner(int cap) {
    if (!__builtin_constant_p(cap)) constant_required();
    return cap + 1;
}
INLINE int outer(int cap) { return inner(cap); }
INLINE int is_constant(int cap) { return __builtin_constant_p(cap); }
static volatile int dynamic_value = 9;
int main(void) {
    int effects = 0;
    if (outer(41) != 42) return 1;
    if (!is_constant(3 + 4)) return 2;
    if (is_constant(dynamic_value)) return 3;
    if (__builtin_constant_p(effects++)) return 4;
    if (effects) return 5;
    return 0;
}
