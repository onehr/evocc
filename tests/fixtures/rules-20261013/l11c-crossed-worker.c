/* L11c: one side of a two-thread crossed-compiler control. Build twice,
 * with -DSIDE=a and -DSIDE=b, by different compilers. Each record keeps
 * count and check in one atomic object; a torn or lost update breaks them. */
#define NAME2(x, y) x##y
#define NAME(x, y) NAME2(x, y)
struct __attribute__((aligned(16))) w16 { long count, check; };
struct w12 { int count, check, spare; };
struct __attribute__((aligned(8))) w8 { int count, check; };
extern struct w16 c16;
extern struct w12 c12;
extern struct w8 c8;

void NAME(work_, SIDE)(int n)
{
    for (int i = 0; i < n; i++) {
        struct w16 o16, n16;
        __atomic_load(&c16, &o16, 0);
        do { n16.count = o16.count + 1; n16.check = o16.check + 2; }
        while (!__atomic_compare_exchange(&c16, &o16, &n16, 1, 5, 0));
        struct w12 o12, n12;
        __atomic_load(&c12, &o12, 0);
        do { n12 = o12; n12.count++; n12.check += 2; }
        while (!__atomic_compare_exchange(&c12, &o12, &n12, 0, 5, 5));
        struct w8 o8, n8;
        __atomic_load(&c8, &o8, 0);
        do { n8.count = o8.count + 1; n8.check = o8.check + 2; }
        while (!__atomic_compare_exchange(&c8, &o8, &n8, 1, 4, 2));
    }
}
