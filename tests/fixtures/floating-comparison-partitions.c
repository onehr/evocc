/* Ordered plus unordered partitions; retain all executed operand reads. */
extern void impossible_float_relation(void);
static void compare(float x, float y)
{
    if ((x == y) && (x != y)) impossible_float_relation();
    if ((x < y) && (x > y)) impossible_float_relation();
    if ((x < y) && (y < x)) impossible_float_relation();
    if (!((x == y) || (x != y))) impossible_float_relation();
    if (!(__builtin_isunordered(x, y) || (x >= y) || (x < y)))
        impossible_float_relation();
    if (!(__builtin_isunordered(y, x) || (x <= y) || (y < x)))
        impossible_float_relation();
    if (!(__builtin_isunordered(x, y) || !__builtin_isunordered(x, y)))
        impossible_float_relation();
}
#ifndef __OPTIMIZE__
void impossible_float_relation(void) { __builtin_abort(); }
#endif
int main(void)
{
#ifdef UNINITIALIZED
    float unknown;
    compare(unknown, 0.0f);
#else
    float nan = __builtin_nanf("");
    compare(0.0f, -0.0f); compare(1.0f, 2.0f); compare(4.0f, 3.0f);
    compare(nan, 0.0f); compare(0.0f, nan); compare(nan, nan);
#endif
    return 0;
}
