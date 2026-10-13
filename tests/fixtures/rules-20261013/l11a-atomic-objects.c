/* L11a: ordinary C operators on _Atomic objects. Single-thread value
 * probe, CASE=0..8; exit 0 is the asserted answer. */
#ifndef CASE
#define CASE 0
#endif
struct pair { int a, b; };

#if CASE == 6
static int take(_Atomic int x) { return x + 1; }
static _Atomic int give(void) { return 5; }
#endif

int main(void)
{
#if CASE == 0 /* integer: assignment, ++/--, every compound operator */
    static _Atomic unsigned x = 3;
    if ((x += 4) != 7 || x++ != 7 || ++x != 9 || x-- != 9 || --x != 7) return 1;
    if ((x -= 2) != 5 || (x |= 8) != 13 || (x &= ~1u) != 12 || (x ^= 3) != 15) return 2;
    if ((x *= 3) != 45 || (x /= 2) != 22 || (x %= 5) != 2) return 3;
    if ((x <<= 4) != 32 || (x >>= 3) != 4 || (x = 9) != 9) return 4;
    _Atomic signed char c = 127;
    if (++c != -128) return 5;
    return x != 9;
#elif CASE == 1 /* pointer: += and ++ scale by the pointee size */
    static int a[4];
    int *_Atomic p = a;
    p += 2; p++; --p; p -= 1;
    return p != a + 1;
#elif CASE == 2 /* _Bool */
    _Atomic _Bool b = 0;
    b = 5;
    if (b != 1) return 1;
    b += 1;
    if (b != 1) return 2;
    b--;
    return b != 0;
#elif CASE == 3 /* double: compound assignment uses one rounded result */
    _Atomic double d = 1.5;
    d += 2.25; d *= 2; d++;
    _Atomic float f = 0.1f;
    f += 0.2f;
    return d != 8.5 || f != 0.1f + 0.2f;
#elif CASE == 4 /* long double (binary128 on Linux, binary64 on Darwin) */
    _Atomic long double q = 1;
    q += 1;
    q *= 3;
    return q != 6;
#elif CASE == 5 /* whole-object record load and store */
    static _Atomic struct pair s;
    struct pair t = {1, 2}, u;
    s = t;
    u = s;
    return u.a != 1 || u.b != 2;
#elif CASE == 6 /* parameters and return values */
    _Atomic int y = give();
    return take(y) != 6;
#elif CASE == 7 /* initialization forms */
    _Atomic int z = 4;
    static _Atomic long w = 7;
    _Atomic int *pz = &(_Atomic int){8};
    return z + w + *pz != 19;
#elif CASE == 8 /* volatile and const atomic reads */
    static volatile _Atomic int v = 3;
    static const _Atomic int k = 4;
    v += k;
    return v != 7;
#else
#error unsupported CASE
#endif
}
