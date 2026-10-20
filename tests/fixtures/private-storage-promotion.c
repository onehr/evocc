/* Private storage crosses joins, loops and whole-record copies. The scalar
 * oracle uses no private memory. Public writes and volatile reads must stay. */
struct word { unsigned long value; };
static struct word published;
static volatile unsigned salt;

__attribute__((noinline)) unsigned cells(unsigned x, int n)
{
    unsigned a[2];
    a[0] = x;
    a[1] = x + 3;
    if (n & 1)
        a[0] += 7;
    else
        a[1] += 9;
    while (n-- > 0) {
        unsigned t = a[0];
        a[0] = a[1];
        a[1] = t + a[1];
        if (n == 2)
            continue;
        a[0] ^= salt;
    }
    return a[0] + a[1];
}

__attribute__((noinline)) unsigned scalar(unsigned x, int n)
{
    unsigned a = x, b = x + 3;
    if (n & 1)
        a += 7;
    else
        b += 9;
    while (n-- > 0) {
        unsigned t = a;
        a = b;
        b = t + b;
        if (n == 2)
            continue;
        a ^= salt;
    }
    return a + b;
}

__attribute__((noinline)) unsigned long records(unsigned x, int n)
{
    struct word a = {x}, b;
    b = a;
    if (n & 1) {
        struct word c = b;
        c.value += 11;
        b = c;
    } else {
        b.value += 17;
    }
    published = b;
    return b.value + a.value;
}

__attribute__((noinline)) unsigned small_cells(unsigned x)
{
    unsigned char a[2];
    a[0] = x;
    a[1] = 255;
    a[0] += a[1];
    return a[0];
}

int main(void)
{
    for (int n = -2; n < 8; ++n) {
        salt = (unsigned)n & 3;
        for (unsigned x = 0; x < 12; ++x) {
            if (cells(x, n) != scalar(x, n))
                return 1;
            unsigned long add = (n & 1) ? 11 : 17;
            if (records(x, n) != 2 * x + add || published.value != x + add)
                return 2;
            if (small_cells(x) != (unsigned char)(x + 255))
                return 3;
        }
    }
    return 0;
}
