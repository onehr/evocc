/* L05: per-lane special values of GNU floating vectors, bit for bit.
 *
 * KIND=32: float x4 (16 bytes); KIND=64: double x2 (16 bytes);
 * KIND=322: float x2 (8 bytes). Lane k of the left operand of pair (i, j)
 * is T[(i + k) % N] and lane k of the right operand is T[(j + 3k) % N],
 * over every ordered pair of the special-value table T: signed zeros,
 * subnormals, extremes, infinities, quiet and signaling NaNs with payloads.
 *
 * Each operation gets one 64-bit hash of the result bits of every lane, in
 * pair and lane order. Under -DFLAGS, the hash also takes the sticky flags
 * that each vector operation raises; it needs -frounding-math, and MODE (0
 * nearest, 1 upward, 2 downward, 3 toward zero) selects the rounding
 * direction. Without FLAGS, MODE must be 0.
 *
 * A lane of + or * with two NaN operands is not hashed: the scalar rule
 * selects the first signaling operand, else the first quiet one, and a
 * compiler can commute these operations (GCC 14 does at -O2 for some
 * shapes). Instead, the line "nan2" counts such lanes whose result differs
 * from the scalar rule, which this file computes from the bits.
 *
 * Conversions use their own tables, with values in the defined range of
 * each C conversion. No library call is made except <fenv.h> and printf.
 *
 * -DSAMPLE keeps the rows i that are multiples of the lane count. Every
 * value of T is then a left operand in one lane, beside every right
 * operand, so the sample still has every pair of values. -DROW_FIRST=f
 * and -DROW_STEP=s keep the rows f, f + s, ... instead. -DPART=1 selects
 * the arithmetic and nan2 lines, -DPART=2 the comparisons and -DPART=3 the
 * conversions. In parts 1 and 2, -DOP=n keeps operation n alone (0 is the
 * first). The EvoCC interpreters run one operation of the sample at a
 * time, within their step budget.
 *
 * Without CHECK, main prints one line per operation:
 *   KIND=k MODE=m NAME HASH
 * With -DCHECK and -DEXPECT=h0,h1,..., main compares the hashes in line
 * order instead and returns 1 + the index of the first difference, or 0.
 * Build references with -ffp-contract=off. */
#ifdef FLAGS
#include <fenv.h>
#pragma STDC FENV_ACCESS ON
#endif
#ifndef CHECK
#include <stdio.h>
#endif
#ifndef KIND
#define KIND 32
#endif
#ifndef MODE
#define MODE 0
#endif
#if MODE != 0 && !defined(FLAGS)
#error MODE needs FLAGS
#endif
#ifndef PART
#define PART 0
#endif
#ifdef OP
#define SELECTED(w) ((w) == OP)
#else
#define SELECTED(w) 1
#endif

typedef unsigned long long u64;

#if KIND == 64
typedef double T;
typedef unsigned long long U;
typedef long long S;
#define LANES 2
#define SIGN 0x8000000000000000ull
#define QUIET 0x0008000000000000ull
#define EXPONENT 0x7ff0000000000000ull
#else
typedef float T;
typedef unsigned U;
typedef int S;
#if KIND == 322
#define LANES 2
#else
#define LANES 4
#endif
#define SIGN 0x80000000u
#define QUIET 0x00400000u
#define EXPONENT 0x7f800000u
#endif

typedef T V __attribute__((vector_size(LANES * sizeof(T))));
typedef __typeof__((V){0} < (V){0}) MV;
typedef S SV __attribute__((vector_size(LANES * sizeof(T))));
typedef U UV __attribute__((vector_size(LANES * sizeof(T))));
typedef union { V v; T t[LANES]; U u[LANES]; } VB;
typedef union { MV v; U u[LANES]; } MB;
typedef union { SV v; S s[LANES]; U u[LANES]; } SB;
typedef union { UV v; U u[LANES]; } UB;

#if KIND == 64
static const U table[] = {
    0, 0x8000000000000000ull, 1, 0x000fffffffffffffull, 0x0010000000000000ull,
    0x3ff0000000000000ull, 0x3ff0000000000001ull, 0x4008000000000000ull,
    0x3fb999999999999aull, 0x7fefffffffffffffull, 0x7ff0000000000000ull,
    0xfff0000000000000ull, 0x7ff8000000000011ull, 0xfff8000000000022ull,
    0x7ff0000000000011ull, 0xfff0000000000022ull, 0xbff8000000000000ull,
    0x4630000000000000ull, 0x39b0000000000000ull};
/* f64 -> s64, in the defined range. */
static const U convert_in[] = {
    0, 0x8000000000000000ull, 0x3fe0000000000000ull, 0xbfe0000000000000ull,
    0x3ff8000000000000ull, 0xc004000000000000ull, 0x400ffae147ae147bull,
    0x43dfffffffffffffull, 0xc3e0000000000000ull, 0x4340000000000001ull,
    0x0000000000000001ull, 0x3e45798ee2308c3aull};
/* s64 -> f64: rounding of large integers. */
static const U integer_in[] = {
    0, 1, 0xffffffffffffffffull, 0x0020000000000001ull,
    0xffdfffffffffffffull, 0x7fffffffffffffffull, 0x8000000000000000ull,
    0x003fffffffffffffull, 3, 0x7ffffffffffffc00ull, 0x7ffffffffffffe00ull,
    0x0020000000000003ull};
#else
static const U table[] = {
    0, 0x80000000u, 1, 0x007fffffu, 0x00800000u, 0x3f800000u, 0x3f800001u,
    0x40400000u, 0x3dcccccdu, 0x7f7fffffu, 0x7f800000u, 0xff800000u,
    0x7fc00011u, 0xffc00022u, 0x7f800011u, 0xff800022u, 0xbfc00000u,
    0x71800000u, 0x0d800000u};
/* f32 -> s32, in the defined range. */
static const U convert_in[] = {
    0, 0x80000000u, 0x3f000000u, 0xbf000000u, 0x3fc00000u, 0xc0200000u,
    0x407f5c29u, 0x4effffffu, 0xcf000000u, 0x4b800001u, 0x00000001u,
    0x0d800000u};
/* s32 -> f32: rounding of large integers. */
static const U integer_in[] = {
    0, 1, 0xffffffffu, 0x01000001u, 0xfeffffffu, 0x7fffffffu, 0x80000000u,
    0x01ffffffu, 3, 0x7fffffc0u, 0x7fffff40u, 0x01000003u};
#endif
#define COUNT (sizeof table / sizeof table[0])
#define CONVERT_COUNT (sizeof convert_in / sizeof convert_in[0])
#define INTEGER_COUNT (sizeof integer_in / sizeof integer_in[0])
#ifndef ROW_FIRST
#define ROW_FIRST 0
#endif
#ifndef ROW_STEP
#ifdef SAMPLE
#define ROW_STEP LANES
#else
#define ROW_STEP 1
#endif
#endif

/* float x2 <-> double x2, over the float and double tables. */
typedef float f2 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef union { f2 v; unsigned u[2]; } F2B;
typedef union { d2 v; unsigned long long u[2]; } D2B;
static const unsigned widen_in[] = {
    0, 0x80000000u, 1, 0x007fffffu, 0x3f800001u, 0x7f7fffffu, 0x7f800000u,
    0xff800000u, 0x7fc00011u, 0xffc00022u, 0x7f800011u, 0xff800022u,
    0x7fffffffu, 0x7fbfffffu};
static const unsigned long long narrow_in[] = {
    0, 0x8000000000000000ull, 0x3ff0000010000000ull, 0x3ff0000030000000ull,
    0x3ff0000008000000ull, 0x47efffffe0000000ull, 0x47efffffefffffffull,
    0x36a0000000000000ull, 0x3690000000000000ull, 0x3e70000000000000ull,
    0x7ff8000000000123ull, 0xfff0000000000011ull, 0xfff0000000000000ull,
    0x7ff7ffffffffffffull, 0x0000000000000001ull, 0xb810000000000000ull};

static u64 hash;
/* One multiply and one shift per value keeps the interpreters' step count
 * small; a changed bit still changes the hash. */
#define MIX(value)                                                           \
    (hash = (hash ^ (u64)(value)) * 0x100000001b3ull, hash ^= hash >> 29)

#ifdef FLAGS
static int raised(void)
{
    int bits = fetestexcept(FE_ALL_EXCEPT), r = 0;
    r |= bits & FE_INVALID ? 1 : 0;
    r |= bits & FE_DIVBYZERO ? 2 : 0;
    r |= bits & FE_OVERFLOW ? 4 : 0;
    r |= bits & FE_UNDERFLOW ? 8 : 0;
    r |= bits & FE_INEXACT ? 16 : 0;
    return r;
}
#define BEGIN() feclearexcept(FE_ALL_EXCEPT)
#define END() MIX(raised())
#else
#define BEGIN() ((void)0)
#define END() ((void)0)
#endif

#define IS_NAN(bits) (((bits) & ~(U)SIGN) > (U)EXPONENT)
#define IS_SIGNALING(bits) (IS_NAN(bits) && !((bits) & (U)QUIET))

/* The scalar rule for two NaN operands of an arithmetic operation. */
static U nan_rule(U a, U b)
{
    if (IS_SIGNALING(a))
        return a | (U)QUIET;
    if (IS_SIGNALING(b))
        return b | (U)QUIET;
    return a;
}

__attribute__((noinline)) static V arith(int which, V a, V b)
{
    switch (which) {
    case 0: return a + b;
    case 1: return a - b;
    case 2: return a * b;
    case 3: return a / b;
    case 4: return -a;
    case 5: return +a;
#if LANES == 4
    default: return __builtin_shufflevector(a, b, 3, 4, 1, 6);
#else
    default: return __builtin_shufflevector(a, b, 1, 2);
#endif
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

__attribute__((noinline)) static SV to_signed(V a) { return __builtin_convertvector(a, SV); }
__attribute__((noinline)) static V from_signed(SV a) { return __builtin_convertvector(a, V); }
__attribute__((noinline)) static V from_unsigned(UV a) { return __builtin_convertvector(a, V); }
__attribute__((noinline)) static UV to_unsigned(V a) { return __builtin_convertvector(a, UV); }
__attribute__((noinline)) static d2 widen(f2 a) { return __builtin_convertvector(a, d2); }
__attribute__((noinline)) static f2 narrow(d2 a) { return __builtin_convertvector(a, f2); }

#ifdef CHECK
static const u64 expect[] = {EXPECT};
#else
static const char *const names[] = {
    "add", "sub", "mul", "div", "neg", "plus", "shuffle", "nan2", "eq", "ne",
    "lt", "le", "gt", "ge", "to_signed", "from_signed", "from_unsigned",
    "to_unsigned", "widen", "narrow"};
#endif
static int produced;

/* Close the line of operation `index`. */
static int finish(int index)
{
#ifdef CHECK
    (void)index;
    if (produced >= (int)(sizeof expect / sizeof expect[0]) || expect[produced] != hash)
        return 1 + produced;
#else
    printf("KIND=%d MODE=%d %s %016llx\n", KIND, MODE, names[index], hash);
#endif
    ++produced;
    hash = 0xcbf29ce484222325ull;
    return 0;
}

/* The unions live outside the loops: an interpreter keeps every object
 * that it creates, so a loop allocates nothing it does not need. */
static VB a, b, r;
static MB m;

static int arithmetic(void)
{
    int bad, nan2 = 0;
    for (int w = 0; w < 7; ++w) {
        if (!SELECTED(w))
            continue;
        for (unsigned i = ROW_FIRST; i < COUNT; i += ROW_STEP)
            for (unsigned j = 0; j < COUNT; ++j) {
                for (unsigned k = 0; k < LANES; ++k) {
                    a.u[k] = table[(i + k) % COUNT];
                    b.u[k] = table[(j + 3 * k) % COUNT];
                }
                BEGIN();
                r.v = arith(w, a.v, b.v);
                END();
                for (unsigned k = 0; k < LANES; ++k) {
                    if ((w == 0 || w == 2) && IS_NAN(a.u[k]) && IS_NAN(b.u[k])) {
                        nan2 += r.u[k] != nan_rule(a.u[k], b.u[k]);
                        continue;
                    }
                    MIX(r.u[k]);
                }
            }
        if ((bad = finish(w)))
            return bad;
    }
#ifdef OP
    /* nan2 counts the lanes of add and mul alone. */
    if (OP != 0 && OP != 2)
        return 0;
#endif
    MIX(nan2);
    return finish(7);
}

static int comparisons(void)
{
    int bad;
    for (int w = 0; w < 6; ++w) {
        if (!SELECTED(w))
            continue;
        for (unsigned i = ROW_FIRST; i < COUNT; i += ROW_STEP)
            for (unsigned j = 0; j < COUNT; ++j) {
                for (unsigned k = 0; k < LANES; ++k) {
                    a.u[k] = table[(i + k) % COUNT];
                    b.u[k] = table[(j + 3 * k) % COUNT];
                }
                BEGIN();
                m.v = compare(w, a.v, b.v);
                END();
                for (unsigned k = 0; k < LANES; ++k)
                    MIX(m.u[k]);
            }
        if ((bad = finish(8 + w)))
            return bad;
    }
    return 0;
}

static int conversions(void)
{
    int bad;
    for (unsigned i = 0; i < CONVERT_COUNT; ++i) {
        VB a;
        SB r;
        for (unsigned k = 0; k < LANES; ++k)
            a.u[k] = convert_in[(i + k) % CONVERT_COUNT];
        BEGIN();
        r.v = to_signed(a.v);
        END();
        for (unsigned k = 0; k < LANES; ++k)
            MIX(r.u[k]);
    }
    if ((bad = finish(14)))
        return bad;
    for (unsigned i = 0; i < INTEGER_COUNT; ++i) {
        SB a;
        VB r;
        for (unsigned k = 0; k < LANES; ++k)
            a.u[k] = integer_in[(i + k) % INTEGER_COUNT];
        BEGIN();
        r.v = from_signed(a.v);
        END();
        for (unsigned k = 0; k < LANES; ++k)
            MIX(r.u[k]);
    }
    if ((bad = finish(15)))
        return bad;
    for (unsigned i = 0; i < INTEGER_COUNT; ++i) {
        UB a;
        VB r;
        for (unsigned k = 0; k < LANES; ++k)
            a.u[k] = integer_in[(i + 2 * k) % INTEGER_COUNT];
        BEGIN();
        r.v = from_unsigned(a.v);
        END();
        for (unsigned k = 0; k < LANES; ++k)
            MIX(r.u[k]);
    }
    if ((bad = finish(16)))
        return bad;
    for (unsigned i = 0; i < CONVERT_COUNT; ++i) {
        VB a;
        UB r;
        /* Lanes that are negative after truncation are outside the
         * unsigned range; take their magnitudes. */
        for (unsigned k = 0; k < LANES; ++k)
            a.u[k] = convert_in[(i + k) % CONVERT_COUNT] & ~(U)SIGN;
        BEGIN();
        r.v = to_unsigned(a.v);
        END();
        for (unsigned k = 0; k < LANES; ++k)
            MIX(r.u[k]);
    }
    if ((bad = finish(17)))
        return bad;
    for (unsigned i = 0; i < sizeof widen_in / sizeof widen_in[0]; ++i) {
        F2B a;
        D2B r;
        a.u[0] = widen_in[i];
        a.u[1] = widen_in[(i + 5) % (sizeof widen_in / sizeof widen_in[0])];
        BEGIN();
        r.v = widen(a.v);
        END();
        MIX(r.u[0]);
        MIX(r.u[1]);
    }
    if ((bad = finish(18)))
        return bad;
    for (unsigned i = 0; i < sizeof narrow_in / sizeof narrow_in[0]; ++i) {
        D2B a;
        F2B r;
        a.u[0] = narrow_in[i];
        a.u[1] = narrow_in[(i + 7) % (sizeof narrow_in / sizeof narrow_in[0])];
        BEGIN();
        r.v = narrow(a.v);
        END();
        MIX(r.u[0]);
        MIX(r.u[1]);
    }
    return finish(19);
}

int main(void)
{
    int bad = 0;
    hash = 0xcbf29ce484222325ull;
#ifdef FLAGS
    static const int modes[] = {FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO};
    if (fesetround(modes[MODE]))
        return 255;
#endif
    if (PART == 0 || PART == 1)
        bad = arithmetic();
    if (!bad && (PART == 0 || PART == 2))
        bad = comparisons();
    if (!bad && (PART == 0 || PART == 3))
        bad = conversions();
    return bad;
}
