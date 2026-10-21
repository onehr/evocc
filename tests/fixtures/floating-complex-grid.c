/* Special-value grid for complex multiplication and division (L04).
 *
 * Each of the four operand parts takes each of 14 values: +0 and -0, the
 * smallest subnormal, 1, 3 and the largest finite value with both signs,
 * +infinity and -infinity, and a quiet NaN of each sign with its own
 * payload. That gives 14^4 = 38,416 operand sets.
 *
 * KIND=32, 64 or 128 selects float, double or long double _Complex.
 * FORM=0 uses the operators * and /.
 * FORM=1 calls the target's runtime helper through a volatile pointer.
 *        Only the reference compilers build this form.
 * FORM=2 takes the results from static initializers, so translation-time
 *        folding computes them. It has no status flags and runs only
 *        round-to-nearest.
 * MODES=1 runs round-to-nearest; MODES=4 also runs upward, downward and
 * toward zero. MODE=M runs rounding mode M only.
 * SAMPLE runs only the 50 operand sets of SAMPLE_CELLS, a sample that the
 * interpreters can run: 42 sets that give each part each value at least
 * twice and cover 252 pairs of values in two parts, and 8 sets that reach
 * the recovery branches of the helpers.
 *
 * For each operation and rounding mode, one hash collects the bits of both
 * result parts and another collects the status flags after each operation.
 * Without CHECK, main prints the hashes, one line per mode. With CHECK, the
 * macros EXPECT_MUL_M, EXPECT_DIV_M, FLAGS_MUL_M and FLAGS_DIV_M give the
 * reference hashes of mode M, and main returns a mask: bit 2*M is set for a
 * wrong product and bit 2*M+1 for a wrong quotient. FORM=2 compares bits
 * only. With DUMP, main prints every result instead.
 *
 * Build: CC -std=gnu11 -frounding-math -ffp-contract=off -DKIND=K -DFORM=F
 *        [-DMODES=4 | -DMODE=M] [-DSAMPLE] FILE -o grid */
#include <fenv.h>
#include <stdio.h>
#include <string.h>
#ifndef KIND
#define KIND 64
#endif
#ifndef FORM
#define FORM 0
#endif
#ifndef MODES
#define MODES 1
#endif
#ifdef MODE
#define FIRST MODE
#define LAST (MODE + 1)
#else
#define FIRST 0
#define LAST MODES
#endif
#if FORM == 2 && (FIRST != 0 || LAST != 1)
#error static initializers fold in round-to-nearest only
#endif

#if KIND == 32
#define R float
#define SUB __FLT_DENORM_MIN__
#define BIG __FLT_MAX__
#define POSITIVE_NAN __builtin_nanf("0x123")
#define NEGATIVE_NAN (-__builtin_nanf("0x456"))
#define INF __builtin_inff()
#define HELPER(name) __##name##sc3
#elif KIND == 64
#define R double
#define SUB __DBL_DENORM_MIN__
#define BIG __DBL_MAX__
#define POSITIVE_NAN __builtin_nan("0x123")
#define NEGATIVE_NAN (-__builtin_nan("0x456"))
#define INF __builtin_inf()
#define HELPER(name) __##name##dc3
#elif KIND == 128
#define R long double
#define SUB __LDBL_DENORM_MIN__
#define BIG __LDBL_MAX__
#define POSITIVE_NAN __builtin_nanl("0x123")
#define NEGATIVE_NAN (-__builtin_nanl("0x456"))
#define INF __builtin_infl()
#if __LDBL_MANT_DIG__ == 113
#define HELPER(name) __##name##tc3
#else
#define HELPER(name) __##name##dc3
#endif
#else
#error unsupported KIND
#endif
typedef R _Complex Z;
typedef unsigned long long u64;

#define V0 ((R)0)
#define V1 (-(R)0)
#define V2 ((R)SUB)
#define V3 (-(R)SUB)
#define V4 ((R)1)
#define V5 (-(R)1)
#define V6 ((R)3)
#define V7 (-(R)3)
#define V8 ((R)BIG)
#define V9 (-(R)BIG)
#define V10 ((R)INF)
#define V11 (-(R)INF)
#define V12 ((R)POSITIVE_NAN)
#define V13 ((R)NEGATIVE_NAN)
#define COUNT 14

#define SAMPLE_CELLS(F)                                                            \
    F(0, 0, 0, 0) F(1, 1, 1, 1) F(2, 2, 2, 2) F(3, 3, 3, 3)                        \
    F(4, 4, 4, 4) F(5, 5, 5, 5) F(6, 6, 6, 6) F(7, 7, 7, 7)                        \
    F(8, 8, 8, 8) F(9, 9, 9, 9) F(10, 10, 10, 10) F(11, 11, 11, 11)                \
    F(12, 12, 12, 12) F(13, 13, 13, 13) F(0, 1, 2, 3) F(1, 0, 3, 2)                \
    F(2, 3, 0, 1) F(3, 2, 1, 0) F(4, 5, 6, 7) F(5, 4, 7, 6)                        \
    F(6, 7, 4, 5) F(7, 6, 5, 4) F(8, 9, 10, 11) F(9, 8, 11, 10)                    \
    F(10, 11, 8, 9) F(11, 10, 9, 8) F(0, 2, 12, 13) F(1, 3, 13, 12)                \
    F(12, 13, 0, 2) F(13, 12, 1, 3) F(2, 0, 4, 6) F(3, 1, 5, 7)                    \
    F(4, 6, 2, 0) F(5, 7, 3, 1) F(6, 4, 8, 10) F(7, 5, 9, 11)                      \
    F(8, 10, 6, 4) F(9, 11, 7, 5) F(10, 9, 11, 8) F(11, 8, 10, 9)                  \
    F(0, 12, 13, 1) F(1, 13, 12, 0)                                                \
    F(10, 12, 4, 4) F(4, 4, 12, 11) F(8, 12, 8, 4) F(4, 4, 0, 0)                   \
    F(10, 0, 4, 4) F(4, 4, 10, 0) F(12, 4, 0, 0) F(5, 4, 11, 10)

#if FORM == 1
Z HELPER(mul)(R, R, R, R);
Z HELPER(div)(R, R, R, R);
static Z (*volatile helpers[2])(R, R, R, R) = { HELPER(mul), HELPER(div) };
#endif

#ifdef SAMPLE
#define INDEX(i, j, k, l) { i, j, k, l },
static const unsigned char cells[][4] = { SAMPLE_CELLS(INDEX) };
#define CELLS (sizeof cells / sizeof cells[0])
#endif

#if FORM == 2
#define C(x, y) __builtin_complex((R)(x), (R)(y))
#define CELL(i, j, k, l) OP(C(V##i, V##j), C(V##k, V##l))
#ifdef SAMPLE
#define SAMPLED(i, j, k, l) CELL(i, j, k, l),
#define TABLE SAMPLE_CELLS(SAMPLED)
#else
#define ROW4(i, j, k)                                                              \
    CELL(i, j, k, 0), CELL(i, j, k, 1), CELL(i, j, k, 2), CELL(i, j, k, 3),        \
        CELL(i, j, k, 4), CELL(i, j, k, 5), CELL(i, j, k, 6), CELL(i, j, k, 7),    \
        CELL(i, j, k, 8), CELL(i, j, k, 9), CELL(i, j, k, 10), CELL(i, j, k, 11),  \
        CELL(i, j, k, 12), CELL(i, j, k, 13)
#define ROW3(i, j)                                                                 \
    ROW4(i, j, 0), ROW4(i, j, 1), ROW4(i, j, 2), ROW4(i, j, 3), ROW4(i, j, 4),     \
        ROW4(i, j, 5), ROW4(i, j, 6), ROW4(i, j, 7), ROW4(i, j, 8), ROW4(i, j, 9), \
        ROW4(i, j, 10), ROW4(i, j, 11), ROW4(i, j, 12), ROW4(i, j, 13)
#define ROW2(i)                                                                    \
    ROW3(i, 0), ROW3(i, 1), ROW3(i, 2), ROW3(i, 3), ROW3(i, 4), ROW3(i, 5),        \
        ROW3(i, 6), ROW3(i, 7), ROW3(i, 8), ROW3(i, 9), ROW3(i, 10), ROW3(i, 11),  \
        ROW3(i, 12), ROW3(i, 13)
#define TABLE                                                                      \
    ROW2(0), ROW2(1), ROW2(2), ROW2(3), ROW2(4), ROW2(5), ROW2(6), ROW2(7),         \
        ROW2(8), ROW2(9), ROW2(10), ROW2(11), ROW2(12), ROW2(13)
#endif
#define OP(x, y) ((x) * (y))
static const Z products[] = { TABLE };
#undef OP
#define OP(x, y) ((x) / (y))
static const Z quotients[] = { TABLE };
#undef OP
#else
static volatile R values[COUNT] = { V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13 };
#endif

static u64 mix(u64 hash, u64 word)
{
    return (hash ^ word) * 0x100000001b3ull;
}

static u64 part(u64 hash, R value)
{
    u64 words[(sizeof(R) + 7) / 8] = { 0 };
    memcpy(words, &value, sizeof value);
    for (unsigned i = 0; i < sizeof words / sizeof words[0]; ++i)
        hash = mix(hash, words[i]);
    return hash;
}

#ifdef DUMP
static void show(R value)
{
    u64 words[(sizeof(R) + 7) / 8] = { 0 };
    memcpy(words, &value, sizeof value);
    for (unsigned i = sizeof words / sizeof words[0]; i; --i)
        printf("%016llx", words[i - 1]);
}
#endif

struct hashes {
    u64 bits, flags;
};

/* One operation on the operand set (i, j, k, l), the index-th one. */
static void cell(struct hashes *hash, int division, int mode, unsigned index, unsigned i,
                 unsigned j, unsigned k, unsigned l)
{
    Z result;
    int flags = 0;
    (void)mode;
#if FORM == 2
    (void)i, (void)j, (void)k, (void)l;
    result = division ? quotients[index] : products[index];
#else
    (void)index;
    Z x = __builtin_complex(values[i], values[j]);
    Z y = __builtin_complex(values[k], values[l]);
    feclearexcept(FE_ALL_EXCEPT);
#if FORM == 1
    result = helpers[division](__real__ x, __imag__ x, __real__ y, __imag__ y);
#else
    result = division ? x / y : x * y;
#endif
    flags = fetestexcept(FE_ALL_EXCEPT);
#endif
    hash->bits = part(hash->bits, __real__ result);
    hash->bits = part(hash->bits, __imag__ result);
    hash->flags = mix(hash->flags, (u64)flags);
#ifdef DUMP
    printf("%d %c %u %u %u %u ", mode, division ? '/' : '*', i, j, k, l);
    show(__real__ result);
    printf(" ");
    show(__imag__ result);
    printf(" %02x\n", flags);
#endif
}

static struct hashes run(int division, int mode)
{
    struct hashes hash = { 0xcbf29ce484222325ull, 0xcbf29ce484222325ull };
#ifdef SAMPLE
    for (unsigned index = 0; index < CELLS; ++index)
        cell(&hash, division, mode, index, cells[index][0], cells[index][1], cells[index][2],
             cells[index][3]);
#else
    unsigned index = 0;
    for (unsigned i = 0; i < COUNT; ++i)
        for (unsigned j = 0; j < COUNT; ++j)
            for (unsigned k = 0; k < COUNT; ++k)
                for (unsigned l = 0; l < COUNT; ++l, ++index)
                    cell(&hash, division, mode, index, i, j, k, l);
#endif
    return hash;
}

#ifdef CHECK
#define WANT(m)                                                                     \
    { { EXPECT_MUL_##m, FLAGS_MUL_##m }, { EXPECT_DIV_##m, FLAGS_DIV_##m } }
#define NONE { { 0, 0 }, { 0, 0 } }
#if FIRST <= 0 && 0 < LAST
#define WANT0 WANT(0)
#else
#define WANT0 NONE
#endif
#if FIRST <= 1 && 1 < LAST
#define WANT1 WANT(1)
#else
#define WANT1 NONE
#endif
#if FIRST <= 2 && 2 < LAST
#define WANT2 WANT(2)
#else
#define WANT2 NONE
#endif
#if FIRST <= 3 && 3 < LAST
#define WANT3 WANT(3)
#else
#define WANT3 NONE
#endif
#endif

int main(void)
{
    static const int modes[4] = { FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO };
    struct hashes hashes[4][2];
    for (int mode = FIRST; mode < LAST; ++mode) {
        if (fesetround(modes[mode]))
            return 255;
        hashes[mode][0] = run(0, mode);
        hashes[mode][1] = run(1, mode);
        fesetround(FE_TONEAREST);
    }
#ifdef CHECK
    static const struct hashes expected[4][2] = { WANT0, WANT1, WANT2, WANT3 };
    int mask = 0;
    for (int mode = FIRST; mode < LAST; ++mode)
        for (int division = 0; division < 2; ++division) {
            struct hashes got = hashes[mode][division], want = expected[mode][division];
            if (got.bits != want.bits || (FORM != 2 && got.flags != want.flags))
                mask |= 1 << (2 * mode + division);
        }
    return mask;
#elif !defined(DUMP)
    for (int mode = FIRST; mode < LAST; ++mode)
        printf("mode %d mul %016llx %016llx div %016llx %016llx\n", mode, hashes[mode][0].bits,
               hashes[mode][0].flags, hashes[mode][1].bits, hashes[mode][1].flags);
    return 0;
#else
    return 0;
#endif
}
