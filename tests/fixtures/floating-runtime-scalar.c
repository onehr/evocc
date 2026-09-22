static volatile float single = 0x1.000002p0f;
static volatile double number = 2.5;

static unsigned long bits(double x) {
    union { double f; unsigned long u; } value = { .f = x };
    return value.u;
}

static double mixed(int a, float b, double c, long d) {
    return (double)a + (double)b + c + (double)d;
}

int main(void) {
    float a = single, b = 0x1.fffffcp-1f;
    if (a * b - 1.0f != 0.0f) return 1;
    if (mixed(3, 1.25f, number, 4) != 10.75) return 2;
    double d = number;
    d += 0.5;
    if (d++ != 3.0 || ++d != 5.0) return 3;
    if ((int)-d != -5 || (unsigned long)d != 5) return 4;
    unsigned __int128 large = (unsigned __int128)1 << 100;
    if ((double)large != 0x1p100 || (unsigned __int128)(double)large != large)
        return 5;
    if (bits(-0.0) != 0x8000000000000000ul || -0.0 != 0.0) return 6;
    volatile double zero = 0.0;
    double inf = d / zero, nan = zero / zero;
    if (!(inf > d) || inf != inf || nan == nan || !(nan != nan)) return 7;
    if (nan < d || nan >= d || !nan || zero) return 8;
    for (int i = 0; i < 3; ++i) d *= 0.5;
    if (d != 0.625) return 9;
    int i = 3;
    i *= 1.5;
    if (i != 4) return 10;
    float empty = {};
    if (empty != 0) return 11;
    _Bool truth = nan;
    return !truth;
}
