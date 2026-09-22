/* NaN, writes and different rounding formats do not share a partition. */
static int count;
static void observed(void) { count++; }
static void unordered(float x, float y)
{
    if (!((x < y) || (x >= y))) observed();
}
static void overwritten(float x, float y)
{
    if ((x < y) && ((x = 3.0f) > y)) observed();
}
static void narrowed(double x, double y)
{
    if ((x < y) && ((float)x >= (float)y)) observed();
}
static void volatile_inputs(volatile float x, volatile float y)
{
    if ((x < y) && (x > y)) observed();
}
int main(void)
{
    unordered(__builtin_nanf(""), 0.0f);
    overwritten(1.0f, 2.0f);
    narrowed(1.0, 1.0 + 0x1p-25);
    volatile_inputs(1.0f, 2.0f);
    return count != 3;
}
