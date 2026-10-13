/* L05 probe: every GNU floating-vector lane equals the scalar operation on
 * the same operands, bit for bit, under the default environment.
 * KIND=32: float x4 (16 bytes); KIND=64: double x2 (16 bytes);
 * KIND=322: float x2 (8 bytes); KIND=328: float x8 (32 bytes, wide).
 * Operations: + - * / unary -, then the six comparisons as lane masks.
 * Operands include signed zeros, subnormals, extremes, infinities and
 * quiet and signaling NaNs with payloads. The exit status counts lanes
 * whose bits differ from the scalar result (0 = lane-wise identity).
 * Build references with -ffp-contract=off. No library call is made. */
#ifndef KIND
#define KIND 32
#endif
#if KIND == 64
typedef double T;
typedef unsigned long U;
typedef long M;
#define LANES 2
#else
typedef float T;
typedef unsigned U;
typedef int M;
#if KIND == 322
#define LANES 2
#elif KIND == 328
#define LANES 8
#else
#define LANES 4
#endif
#endif
typedef T V __attribute__((vector_size(LANES * sizeof(T))));
typedef __typeof__((V){0} < (V){0}) MV;
typedef union { V v; T t[LANES]; U u[LANES]; } VB;
typedef union { MV v; M m[LANES]; } MB;
typedef union { T t; U u; } SB;

#if KIND == 64
static const U table[] = {
    0, 0x8000000000000000ul, 1, 0x000ffffffffffffful, 0x3ff0000000000000ul,
    0x3ff0000000000001ul, 0x7feffffffffffffful, 0x7ff0000000000000ul,
    0xfff0000000000000ul, 0x7ff8000000000011ul, 0xfff8000000000022ul,
    0x7ff0000000000011ul, 0x4008000000000000ul};
#else
static const U table[] = {
    0, 0x80000000u, 1, 0x007fffffu, 0x3f800000u, 0x3f800001u, 0x7f7fffffu,
    0x7f800000u, 0xff800000u, 0x7fc00011u, 0xffc00022u, 0x7f800011u,
    0x40400000u};
#endif
#define COUNT (sizeof table / sizeof table[0])

__attribute__((noinline)) static V arith(int which, V a, V b)
{
    switch (which) {
    case 0: return a + b;
    case 1: return a - b;
    case 2: return a * b;
    case 3: return a / b;
    default: return -a;
    }
}

__attribute__((noinline)) static T scalar(int which, T a, T b)
{
    switch (which) {
    case 0: return a + b;
    case 1: return a - b;
    case 2: return a * b;
    case 3: return a / b;
    default: return -a;
    }
}

__attribute__((noinline)) static MV compare(int which, V a, V b)
{
    switch (which) {
    case 0: return a == b;
    case 1: return a != b;
    case 2: return a < b;
    case 3: return a <= b;
    case 4: return a > b;
    default: return a >= b;
    }
}

__attribute__((noinline)) static M scalar_compare(int which, T a, T b)
{
    int r;
    switch (which) {
    case 0: r = a == b; break;
    case 1: r = a != b; break;
    case 2: r = a < b; break;
    case 3: r = a <= b; break;
    case 4: r = a > b; break;
    default: r = a >= b; break;
    }
    return r ? -1 : 0;
}

int main(void)
{
    int bad = 0;
    if (sizeof(MV) != sizeof(V))
        return 254;
    for (unsigned i = 0; i < COUNT; ++i)
        for (unsigned j = 0; j < COUNT; ++j) {
            VB a, b, r;
            MB m;
            for (unsigned k = 0; k < LANES; ++k) {
                a.u[k] = table[(i + k) % COUNT];
                b.u[k] = table[(j + 3 * k) % COUNT];
            }
            for (int w = 0; w < 5; ++w) {
                r.v = arith(w, a.v, b.v);
                for (unsigned k = 0; k < LANES; ++k) {
                    SB s;
                    s.t = scalar(w, a.t[k], b.t[k]);
                    bad += s.u != r.u[k];
                }
            }
            for (int w = 0; w < 6; ++w) {
                m.v = compare(w, a.v, b.v);
                for (unsigned k = 0; k < LANES; ++k)
                    bad += scalar_compare(w, a.t[k], b.t[k]) != m.m[k];
            }
        }
    return bad > 253 ? 253 : bad;
}
