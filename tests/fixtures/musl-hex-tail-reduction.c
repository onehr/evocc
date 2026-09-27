/* B08d reduction of musl 1.2.6 hexfloat's tail classification, not a strtold
 * replacement. Inputs are positive, normal, with exactly one leading digit.
 * One extra captured nibble is a diagnostic control, not an upstream patch. */
static long double reduce(const char *s, int extra_guard_digit)
{
    unsigned x = 0;
    long double y = 0, scale = 1;
    int dc = 0, tail = 0, exponent = -28;
    for (; *s; ++s) {
        if (*s == '.') continue;
        int d = *s - '0';
        if (dc < 8) x = x * 16 + d;
        else if (dc < 113 / 4 + 1 + extra_guard_digit)
            y += d * (scale /= 16);
        else if (d && !tail) {
            y += 0.5L * scale;
            tail = 1;
        }
        ++dc;
    }
    while (x < 0x80000000U) {
        if (y >= 0.5L) { x += x + 1; y += y - 1; }
        else { x += x; y += y; }
        --exponent;
    }
    /* Both chosen inputs normalize by three bits. This exact scaling replaces
     * only the external scalbnl call, keeping the decisive rounded x+y sum. */
    if (exponent != -31) return 0;
    return (x + y) * 0x1p-31L;
}

int main(void)
{
    const char *a = "1.111111111111111111111111111281";
    const char *b = "1.11111111111111111111111111111";
    const long double old = 0x1.1111111111111111111111111112p0L;
    const long double above = 0x1.1111111111111111111111111113p0L;
    const long double below = 0x1.1111111111111111111111111111p0L;
    return (reduce(a, 0) != old) | ((reduce(b, 0) != old) << 1)
         | ((reduce(a, 1) != above) << 2) | ((reduce(b, 1) != below) << 3);
}
