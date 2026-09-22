extern double fabs(double);
extern float fabsf(float);
extern void impossible_absolute_relation(void);
static int calls;
static double operand(void) { calls++; return -3.0; }
static void compare(double x)
{
    double p = fabs(x), q = 0.0;
    if (p < q) impossible_absolute_relation();
}
static void compare_reversed(double x)
{
    double p = fabs(x), q = -0.0;
    if (q > p) impossible_absolute_relation();
}
#ifndef __OPTIMIZE__
void impossible_absolute_relation(void) { __builtin_abort(); }
#endif
int main(void)
{
    if (fabs(operand()) != 3.0 || calls != 1) return 1;
    if (__builtin_signbit(fabs(-0.0)) || __builtin_signbit(fabsf(-0.0f))) return 2;
    compare(-1.0); compare(-0.0); compare(__builtin_nan(""));
    compare_reversed(-1.0); compare_reversed(__builtin_nan(""));
    /* Nonnegative is not >= 0 when the value is unordered. */
    if (fabs(__builtin_nan("")) >= 0.0) return 3;
    return 0;
}
