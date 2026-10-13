/* K07 probe: contexts that require an integer constant expression, and the
 * folded values. Each CASE compiles one context; main returns 0 when the
 * folded values are the expected lengths. */
static const char array_of_10[] = "this is 10";
static const char *const unchanging_12 = "this is 12!!";
static const char embedded[] = "ab\0cd";
static const char rows[2][4] = { "ab", "cde" };

#if CASE == 0
_Static_assert(__builtin_strlen(array_of_10) == 10, "static array");
_Static_assert(__builtin_strlen(unchanging_12) == 12, "const pointer");
int main(void) { return 0; }
#elif CASE == 1
enum { E10 = __builtin_strlen(array_of_10), E12 = __builtin_strlen(unchanging_12) };
int main(void) { return (E10 == 10 && E12 == 12) ? 0 : 1; }
#elif CASE == 2
static char buffer[__builtin_strlen(array_of_10) + 1];
int main(void) { return sizeof buffer == 11 ? 0 : 1; }
#elif CASE == 3
int main(void)
{
    volatile int n = 10;
    switch (n) {
    case __builtin_strlen(array_of_10): return 0;
    default: return 1;
    }
}
#elif CASE == 4
static const unsigned long n12 = __builtin_strlen(unchanging_12);
int main(void) { return n12 == 12 ? 0 : 1; }
#elif CASE == 5
struct bits { unsigned field : __builtin_strlen(array_of_10); };
int main(void) { return sizeof(struct bits) == 4 ? 0 : 1; }
#elif CASE == 6
int main(void)
{
    return __builtin_choose_expr(__builtin_strlen(array_of_10) == 10, 0, 1);
}
#elif CASE == 7
int main(void)
{
    /* Values only: runtime evaluation of the admitted operands. */
    unsigned long a = __builtin_strlen(array_of_10);
    unsigned long b = __builtin_strlen(unchanging_12);
    unsigned long c = __builtin_strlen(embedded);
    unsigned long d = __builtin_strlen(array_of_10 + 10);
    unsigned long e = __builtin_strlen(rows[1]);
    return (a == 10 && b == 12 && c == 2 && d == 0 && e == 3) ? 0 : 1;
}
#elif CASE == 8
int main(void)
{
    return __builtin_constant_p(__builtin_strlen(array_of_10))
         | (__builtin_constant_p(__builtin_strlen(unchanging_12)) << 1);
}
#endif
