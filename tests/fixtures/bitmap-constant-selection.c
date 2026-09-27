/* Constant-query dispatch must carry the selected path's private memory facts. */
#define AI static inline __attribute__((always_inline))
extern void bitmap_constant_required(void);
AI void clear(unsigned long *p, unsigned n)
{
    if (__builtin_constant_p(n) && n == 1) *p &= ~1UL;
    else if (__builtin_constant_p(n) && n <= 64) *p &= 0;
    else bitmap_constant_required();
}
AI int read_constant(const volatile unsigned long *p)
{
    const unsigned long *q = (const unsigned long *)p;
    unsigned long value = *q;
    return !!(value & 128UL);
}
AI int read_runtime(const volatile unsigned long *p) { return !!(*p & 128UL); }
#define READ(p) ((__builtin_constant_p((unsigned long)(p) != 0) && \
                  (unsigned long)(p) != 0 && \
                  __builtin_constant_p(*(const unsigned long *)(p))) ? \
                  read_constant(p) : read_runtime(p))
AI void set(unsigned long *p) { *p |= 96UL; }
int main(void)
{
    unsigned long bitmap[1];
    bitmap[0] = 0;
    clear(bitmap, 64);
    if (!READ(bitmap)) set(bitmap);
    unsigned weight = __builtin_popcountl(bitmap[0]);
    if (!__builtin_constant_p(weight) || weight != 2) bitmap_constant_required();
    return 0;
}
