/* Reduced from Linux lib/test_bitmap.c:test_bitmap_const_eval. */
extern void constant_assertion_failed(void);
#define CHECK(c) do { if (!(c)) constant_assertion_failed(); } while (0)

int main(void)
{
    unsigned long var = 0;
    unsigned long initvar = 1UL << 2;
    int res;
    var |= 1UL << 25;
    if (var & 1)
        var ^= (0xFUL << 6);
    res = initvar & (0x7FUL << 8);
    CHECK(__builtin_constant_p(res));
    CHECK(res == 0);
    CHECK(__builtin_constant_p(~var));
    CHECK(~var == ~(1UL << 25));
    return 0;
}
