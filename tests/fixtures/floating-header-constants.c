/* The target float.h expands DBL_EPSILON through a binary128 literal. */
#define EPSILON ((double)2.22044604925031308084726333618164062e-16L)
#define LIMIT ((double)1.79769313486231570814527423731704357e+308L)
static double static_epsilon = EPSILON;
static double scale(double x) { return x * EPSILON; }
int main(void) {
    volatile double one = 1.0;
    float single = (float)0x1.000002p0L;
    double negative = (double)-0x1.8p2L;
    if (scale(one) != 0x1p-52 || static_epsilon != 0x1p-52) return 1;
    if (LIMIT != 0x1.fffffffffffffp1023) return 2;
    if (single != 0x1.000002p0f || negative != -6.0) return 3;
    return 0;
}
