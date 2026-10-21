/* Records of at most sixteen bytes with alignment 1, 2 and 4 pass in
 * general registers (EvoMIR 6.4). Under -mstrict-align every access that
 * moves their bytes is aligned to its width (EvoMIR 8.9, D31). Several
 * records share each frame, so that without the rule a slot lies at an
 * offset that only 4 divides. The program needs no library. main returns 0
 * when every value is the expected one; GCC 14 and Apple clang 21 return 0
 * too. */
struct w8 { int a, b; };
struct w12 { int a, b, c; };
struct h6 { short a, b, c; };
struct h14 { short a[7]; };
struct b3 { char a[3]; };
struct b7 { char a[7]; };
struct b13 { char a[13]; };
struct m12 { char c; short s; int i; char d; };

__attribute__((noinline)) int sum_w12(struct w12 a, struct w12 b, struct h6 c)
{
    return a.a + a.c + b.b + c.c;
}

__attribute__((noinline)) int sum_small(struct b3 a, struct b7 b, struct h6 c, struct b13 d)
{
    return a.a[2] + b.a[6] + c.b + d.a[12];
}

__attribute__((noinline)) int sum_mixed(struct m12 a, struct w8 b, struct m12 c)
{
    return a.c + a.s + a.i + a.d + b.b + c.i;
}

__attribute__((noinline)) struct w12 make_w12(int x)
{
    struct w12 r = { x, x + 1, x + 2 };
    return r;
}

__attribute__((noinline)) struct h6 make_h6(short x)
{
    struct h6 r = { x, (short)(x * 2), (short)(x * 3) };
    return r;
}

__attribute__((noinline)) struct h14 make_h14(short x)
{
    struct h14 r;
    for (int i = 0; i < 7; i++)
        r.a[i] = (short)(x + i);
    return r;
}

__attribute__((noinline)) struct b7 make_b7(char x)
{
    struct b7 r;
    for (int i = 0; i < 7; i++)
        r.a[i] = (char)(x + i);
    return r;
}

__attribute__((noinline)) struct b13 make_b13(char x)
{
    struct b13 r;
    for (int i = 0; i < 13; i++)
        r.a[i] = (char)(x + i);
    return r;
}

__attribute__((noinline)) struct w12 pick(struct w12 a, struct w12 b, int first)
{
    return first ? a : b;
}

__attribute__((noinline)) int through(const struct w12 *p, const struct h6 *q)
{
    return sum_w12(*p, p[1], *q);
}

int main(void)
{
    int failures = 0;
    struct w12 x = make_w12(10);
    struct w12 y = make_w12(20);
    struct h6 z = make_h6(3);
    failures += sum_w12(x, y, z) != 10 + 12 + 21 + 9;
    failures += pick(x, y, 0).c != 22;
    failures += pick(x, y, 1).b != 11;

    struct b3 a = { { 1, 2, 3 } };
    struct b7 b = make_b7(4);
    struct b13 d = make_b13(5);
    failures += sum_small(a, b, z, d) != 3 + 10 + 6 + 17;

    struct m12 m = { 1, 2, 3, 4 };
    struct m12 n = { 5, 6, 7, 8 };
    struct w8 w = { 9, 10 };
    failures += sum_mixed(m, w, n) != 1 + 2 + 3 + 4 + 10 + 7;

    struct h14 h = make_h14(30);
    failures += h.a[0] + h.a[6] != 30 + 36;

    /* Records at offsets 4 and 16 of an array, through a pointer. */
    struct w12 array[3] = { { 1, 2, 3 }, { 4, 5, 6 }, { 7, 8, 9 } };
    struct h6 halves[2] = { { 1, 2, 3 }, { 4, 5, 6 } };
    failures += through(&array[1], &halves[1]) != 4 + 6 + 8 + 6;
    return failures;
}
