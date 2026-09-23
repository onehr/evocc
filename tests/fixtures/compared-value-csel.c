/* Full-width, already-evaluated integer selections; no new observation may
 * replace an arm. Run this together with compared-value-selection.c, which
 * covers volatile/atomic/access/overflow boundaries and a guarded array arm. */
long min_s(long a, long b) { return a < b ? a : b; }
long max_s(long a, long b) { return a <= b ? b : a; }
unsigned long min_u(unsigned long a, unsigned long b) { return a < b ? a : b; }
unsigned long max_u(unsigned long a, unsigned long b) { return a <= b ? b : a; }
unsigned long eq(unsigned long a, unsigned long b) { return a == b ? a : b; }
unsigned long ne(unsigned long a, unsigned long b) { return a != b ? a : b; }

int main(void)
{
    volatile unsigned long v[] = {0, 1, 0x7fffffffffffffffUL,
                                 0x8000000000000000UL, ~0UL};
    for (unsigned i = 0; i < 5; ++i) {
        for (unsigned j = 0; j < 5; ++j) {
            unsigned long a = v[i], b = v[j];
            unsigned long ulo = a, uhi = b;
            long slo = (long)a, shi = (long)b;
            if (a > b) { ulo = b; uhi = a; }
            if ((long)a > (long)b) { slo = (long)b; shi = (long)a; }
            if (min_u(a, b) != ulo || max_u(a, b) != uhi)
                return 1;
            if (min_s((long)a, (long)b) != slo || max_s((long)a, (long)b) != shi)
                return 2;
            if (eq(a, b) != b || ne(a, b) != a)
                return 3;
        }
    }
    return 0;
}
