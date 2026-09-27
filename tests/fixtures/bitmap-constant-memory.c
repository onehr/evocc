/* K12b: remaining address-based constant knowledge, not source ICE admission. */
extern void bitmap_constant_required(void);
static inline __attribute__((always_inline)) void clear(unsigned long *p)
{
    *p = 0;
}
static inline __attribute__((always_inline)) void set(unsigned long *p)
{
    *p |= 3UL << 5;
}
static inline __attribute__((always_inline)) int test(unsigned long *p)
{
    return (*p >> 7) & 1;
}
int main(void)
{
    unsigned long bitmap[1];
    clear(bitmap);
    if (!test(bitmap)) set(bitmap);
    int weight = __builtin_popcountl(bitmap[0] & ((1UL << 20) - 1));
    if (!__builtin_constant_p(weight)) bitmap_constant_required();
    if (weight != 2) bitmap_constant_required();
    return 0;
}
