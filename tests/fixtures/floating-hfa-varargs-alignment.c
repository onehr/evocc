/* Reduced from GCC torture pr92904: HFA register exhaustion and stack copy. */
#ifndef RECORD_ALIGN
#define RECORD_ALIGN 32
#endif
#ifndef MEMBER_ALIGN
#define MEMBER_ALIGN 8
#endif
struct __attribute__((aligned(RECORD_ALIGN))) H4 {
    double a __attribute__((aligned(MEMBER_ALIGN)));
    double b, c, d;
};
struct H4 take_hfa(int count, ...);

#ifndef CALLER_ONLY
__attribute__((noinline)) struct H4 take_hfa(int count, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, count);
    while (count--)
        __builtin_va_arg(ap, double);
    struct H4 r = __builtin_va_arg(ap, struct H4);
    __builtin_va_end(ap);
    return r;
}
#endif

#ifndef WORKER_ONLY
int main(void)
{
    struct H4 v = {1.25, 2.75, -3.5, -2.0};
    struct H4 r;
#define CHECK(N, EXPR) do { r = (EXPR); \
    if (r.a != v.a || r.b != v.b || r.c != v.c || r.d != v.d) \
        return (N) + 1; } while (0)
    CHECK(0, take_hfa(0, v));
    CHECK(1, take_hfa(1, 0.0, v));
    CHECK(2, take_hfa(2, 0.0, 0.0, v));
    CHECK(3, take_hfa(3, 0.0, 0.0, 0.0, v));
    CHECK(4, take_hfa(4, 0.0, 0.0, 0.0, 0.0, v));
    CHECK(5, take_hfa(5, 0.0, 0.0, 0.0, 0.0, 0.0, v));
    CHECK(6, take_hfa(6, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, v));
    CHECK(7, take_hfa(7, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, v));
    CHECK(8, take_hfa(8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, v));
    CHECK(9, take_hfa(9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, v));
    return 0;
}
#endif
