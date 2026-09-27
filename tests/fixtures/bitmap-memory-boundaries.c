/* Native controls for alias invalidation, selected effects and loop entries. */
static __attribute__((noinline)) void change(unsigned long *p, unsigned long v)
{ *p = v; }
static void cleanup(unsigned long **p) { **p = 9; }
static inline __attribute__((always_inline)) int early(unsigned long *p, int c)
{ if (c) return *p; *p = 8; return 9; }
int main(void)
{
    for (unsigned n = 0; n < 128; n++) {
        unsigned long a[2];
        a[0] = 3; a[1] = 5;
        change(a + (n & 1), n);
        if (a[n & 1] != n || a[!(n & 1)] != ((n & 1) ? 3 : 5)) return 1;
        volatile unsigned choice = n & 1;
        a[0] = 3;
        choice ? ({ a[0] = 8; 0; }) : ({ if (a[0] != 3) return 2; 0; });
        if (a[0] != (choice ? 8 : 3)) return 3;
        a[0] = 3;
        choice && ({ a[0] = 8; 0; });
        if (a[0] != (choice ? 8 : 3)) return 4;
        unsigned char *bytes = (unsigned char *)a;
        *bytes = 9;
        if (a[0] != 9) return 5;
        volatile unsigned long *access = a;
        *access = 11;
        if (a[0] != 11) return 6;
        { unsigned long *p __attribute__((cleanup(cleanup))) = a; a[0] = 3; }
        if (a[0] != 9) return 7;
        a[0] = 3;
        int r = early(a, choice);
        if (r != (choice ? 3 : 9) || a[0] != (choice ? 3 : 8)) return 8;
        a[0] = 3;
        for (unsigned i = 0; i < 4; i++) a[0] += i;
        if (a[0] != 9) return 9;
    }
    return 0;
}
