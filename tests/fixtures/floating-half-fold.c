/* L03: translation-time evaluation of the half formats.
 *
 * KIND=0: _Float16; KIND=1: __fp16 (both binary16); KIND=2: __bf16. The
 * values V are the special values of floating-half-grid.c that a constant
 * expression can spell: every value of its table except the two signaling
 * NaNs, with the same NaN payloads. The conversions take lists of ties, the
 * values beside them, overflow thresholds, subnormal ties, NaNs with
 * payloads and integers of each width.
 *
 * -DFOLD=1 computes every result in a static initializer, so the compiler
 * evaluates it at translation time. -DFOLD=0 computes the same results at
 * run time from volatile operands. Both forms give one 64-bit hash per
 * operation, in the same order, so a compiler whose folding agrees with its
 * own run time prints the same lines for both forms.
 *
 * Without CHECK, main prints one line per operation: KIND=k NAME HASH.
 * With -DCHECK and -DEXPECT=h0,h1,..., main compares the hashes in line
 * order instead and returns 1 + the index of the first difference, or 0.
 * -DOP=n keeps operation n alone (0 is the first). Build with
 * -ffp-contract=off. */
#ifndef CHECK
#include <stdio.h>
#endif
#ifndef KIND
#define KIND 0
#endif
#ifndef FOLD
#define FOLD 1
#endif
#ifdef OP
#define SELECTED(w) ((w) == OP)
#else
#define SELECTED(w) 1
#endif

typedef unsigned long long u64;
typedef unsigned short u16;

#if KIND == 0
typedef _Float16 H;
typedef __bf16 HO;
#elif KIND == 1
typedef __fp16 H;
typedef __bf16 HO;
#elif KIND == 2
typedef __bf16 H;
typedef _Float16 HO;
#else
#error unsupported KIND
#endif
#define BF (KIND == 2)

typedef union { H h; u16 u; } HB;
typedef union { HO h; u16 u; } OB;
typedef union { float f; unsigned u; } FB;
typedef union { double d; u64 u; } DB;

/* Binary16 values: the table of floating-half-grid.c without 0x7c11 and
 * 0xfd22. Bfloat16 values: its table without 0x7f91 and 0xffa2. */
#define F16_0 0.0f
#define F16_1 -0.0f
#define F16_2 0x1p-24f
#define F16_3 0x1.ff8p-15f
#define F16_4 0x1p-14f
#define F16_5 1.0f
#define F16_6 0x1.004p0f
#define F16_7 1.5f
#define F16_8 3.0f
#define F16_9 0x1.998p-4f
#define F16_10 65504.0f
#define F16_11 __builtin_inff()
#define F16_12 (-__builtin_inff())
#define F16_13 __builtin_nanf("")
#define F16_14 __builtin_nanf("0x22000")
#define F16_15 (-__builtin_nanf("0x44000"))
#define F16_16 -1.5f
#define F16_17 256.0f
#define F16_18 0x1p-8f
#define F16_19 -4.0f
#define F16_20 0x1.004p-15f
#define F16_21 32768.0f
#define BF16_0 0.0f
#define BF16_1 -0.0f
#define BF16_2 0x1p-133f
#define BF16_3 0x1.fcp-127f
#define BF16_4 0x1p-126f
#define BF16_5 1.0f
#define BF16_6 0x1.02p0f
#define BF16_7 1.5f
#define BF16_8 3.0f
#define BF16_9 0x1.9ap-4f
#define BF16_10 0x1.fep127f
#define BF16_11 __builtin_inff()
#define BF16_12 (-__builtin_inff())
#define BF16_13 __builtin_nanf("")
#define BF16_14 __builtin_nanf("0x110000")
#define BF16_15 (-__builtin_nanf("0x220000"))
#define BF16_16 -1.5f
#define BF16_17 256.0f
#define BF16_18 0x1p-8f
#define BF16_19 -4.0f
#define BF16_20 0x1.04p-127f
#define BF16_21 0x1p127f

#if BF
#define S(i) BF16_##i
#define SO(i) F16_##i
#else
#define S(i) F16_##i
#define SO(i) BF16_##i
#endif
/* The value i of this format, and of the other half format. */
#define V(i) ((H)S(i))
#define VO(i) ((HO)SO(i))
#define EACH(F)                                                                \
    F(0), F(1), F(2), F(3), F(4), F(5), F(6), F(7), F(8), F(9), F(10), F(11),  \
        F(12), F(13), F(14), F(15), F(16), F(17), F(18), F(19), F(20), F(21)
#define COLUMNS(F, x)                                                          \
    F(x, V(0)), F(x, V(1)), F(x, V(2)), F(x, V(3)), F(x, V(4)), F(x, V(5)),    \
        F(x, V(6)), F(x, V(7)), F(x, V(8)), F(x, V(9)), F(x, V(10)),           \
        F(x, V(11)), F(x, V(12)), F(x, V(13)), F(x, V(14)), F(x, V(15)),       \
        F(x, V(16)), F(x, V(17)), F(x, V(18)), F(x, V(19)), F(x, V(20)),       \
        F(x, V(21))
#define PAIRS(F)                                                               \
    COLUMNS(F, V(0)), COLUMNS(F, V(1)), COLUMNS(F, V(2)), COLUMNS(F, V(3)),    \
        COLUMNS(F, V(4)), COLUMNS(F, V(5)), COLUMNS(F, V(6)),                  \
        COLUMNS(F, V(7)), COLUMNS(F, V(8)), COLUMNS(F, V(9)),                  \
        COLUMNS(F, V(10)), COLUMNS(F, V(11)), COLUMNS(F, V(12)),               \
        COLUMNS(F, V(13)), COLUMNS(F, V(14)), COLUMNS(F, V(15)),               \
        COLUMNS(F, V(16)), COLUMNS(F, V(17)), COLUMNS(F, V(18)),               \
        COLUMNS(F, V(19)), COLUMNS(F, V(20)), COLUMNS(F, V(21))

/* Binary32 inputs of the narrowing: ties of both formats and the values
 * beside them, overflow thresholds, subnormal ties, infinities and NaNs. */
#define NARROW32                                                               \
    0.0f, -0.0f, 1.0f, 0x1.002p0f, 0x1.006p0f, 0x1.002002p0f, 0x1.001ffep0f,   \
        0x1.ffcp15f, 0x1.ffep15f, 0x1.ffdffep15f, 0x1p16f, 0x1p-25f,           \
        0x1.000002p-25f, 0x1.8p-24f, 0x1.ffcp-15f, 0x1.ffep-15f, 0x1.01p0f,    \
        0x1.03p0f, 0x1.010002p0f, 0x1.00fffep0f, 0x1.fep127f, 0x1.ffp127f,     \
        0x1.fefffep127f, 0x1p-134f, 0x1.0001p-134f, 0x1.8p-133f, 0x1.fcp-127f, \
        0x1.fep-127f, __builtin_inff(), -__builtin_inff(), __builtin_nanf(""), \
        -__builtin_nanf("0x12345"), 0x1.fffffep127f, 0x1p-149f, -0x1p16f,      \
        0x1.99999ap-4f, -0x1.002p0f, -0x1.01p0f
/* Binary64 inputs: the same thresholds, values that a rounding through
 * binary32 would round twice, and the least subnormal. */
#define NARROW64                                                               \
    0.0, -0.0, 1.0, 0x1.002p0, 0x1.0020000001p0, 0x1.001fffffffffffp0,        \
        0x1.00200004p0, 0x1.01p0, 0x1.0100000001p0, 0x1.00ffffffffffffp0,      \
        0x1.01000004p0, 65520.0, 0x1.ffdfffffffffffp15, 0x1.ffp127,            \
        0x1.feffffffffffffp127, 0x1p-25, 0x1.0000000000001p-25, 0x1p-134,      \
        0x1.0000000000001p-134, 0x1p-1074, 0x1.fffffffffffffp1023,            \
        __builtin_inf(), -__builtin_inf(), __builtin_nan(""),                  \
        -__builtin_nan("0x123"), 0.1, -0x1.0020000000001p0
/* Values with a defined conversion to int, each through F. */
#if BF
#define TO_INT(F)                                                              \
    F(0.0f), F(-0.0f), F(0.5f), F(-0.5f), F(1.5f), F(-1.5f), F(2.5f),          \
        F(256.0f), F(0x1.fep30f), F(-0x1p31f), F(0x1p-133f), F(0x1.fcp-1f)
#else
#define TO_INT(F)                                                              \
    F(0.0f), F(-0.0f), F(0.5f), F(-0.5f), F(1.5f), F(-1.5f), F(2.5f),          \
        F(65504.0f), F(-65504.0f), F(0x1p-24f), F(0x1.ffcp-1f), F(2048.0f)
#endif
#define HALF(x) (H)(x)
#define TRUNCATE(x) (int)(H)(x)
#define INT32                                                                  \
    0, 1, -1, 2049, 2051, -2049, 0x7fffffff, -0x7fffffff - 1, 65504, 65519,    \
        65520, 65535, 257, 259, 0x01010001, 0x00ff8000, 0x00ff7fff,            \
        0x7f7fffff, 4097, 3
#define INT64                                                                  \
    0ll, 1ll, -1ll, 0x7fffffffffffffffll, -0x7fffffffffffffffll - 1,           \
        0xfff0ll, 0xffefll, 0x100000000ll, -0x100000000ll, 2049ll,             \
        0x0020000000000001ll, 0x1010000000000001ll, 0x00ff800000000001ll,      \
        -65520ll, 65504ll

#define COUNT(a) (sizeof (a) / sizeof (a)[0])

static u64 hash = 0xcbf29ce484222325ull;
#define MIX(value) (hash = (hash ^ (u64)(value)) * 0x100000001b3ull, hash ^= hash >> 29)

#ifdef CHECK
static const u64 expect[] = {EXPECT};
#else
static const char *const names[] = {
    "add", "sub", "mul", "div", "neg", "eq", "ne", "lt", "le", "gt", "ge",
    "widen32", "widen64", "narrow32", "narrow64", "to_int", "from_int",
    "from_long", "from_ulong", "to_other", "from_other"};
#endif
static int produced;

/* Close the line of operation `index`. */
static int finish(int index)
{
#ifdef CHECK
    (void)index;
    if (produced >= (int)COUNT(expect) || expect[produced] != hash)
        return 1 + produced;
#else
    printf("KIND=%d %s %016llx\n", KIND, names[index], hash);
#endif
    ++produced;
    hash = 0xcbf29ce484222325ull;
    return 0;
}

#define ADD(x, y) (H)((x) + (y))
#define SUB(x, y) (H)((x) - (y))
#define MUL(x, y) (H)((x) * (y))
#define DIV(x, y) (H)((x) / (y))
#define EQ(x, y) ((x) == (y))
#define NE(x, y) ((x) != (y))
#define LT(x, y) ((x) < (y))
#define LE(x, y) ((x) <= (y))
#define GT(x, y) ((x) > (y))
#define GE(x, y) ((x) >= (y))
#define NEG(i) (H)(-V(i))
#define WIDEN32(i) (float)V(i)
#define WIDEN64(i) (double)V(i)
#define TO_OTHER(i) (HO)V(i)
#define FROM_OTHER(i) (H)VO(i)

static HB r;
static OB o;
static FB f;
static DB d;

#if FOLD
static const H add_r[] = {PAIRS(ADD)}, sub_r[] = {PAIRS(SUB)}, mul_r[] = {PAIRS(MUL)},
               div_r[] = {PAIRS(DIV)}, neg_r[] = {EACH(NEG)};
static const int compare_r[6][22 * 22] = {{PAIRS(EQ)}, {PAIRS(NE)}, {PAIRS(LT)},
                                         {PAIRS(LE)}, {PAIRS(GT)}, {PAIRS(GE)}};
static const float widen32_r[] = {EACH(WIDEN32)};
static const double widen64_r[] = {EACH(WIDEN64)};
static const H narrow32_r[] = {NARROW32}, narrow64_r[] = {NARROW64};
static const int to_int_r[] = {TO_INT(TRUNCATE)};
static const H from_int_r[] = {INT32}, from_long_r[] = {INT64};
static const H from_ulong_r[] = {
    (unsigned long long)0ll, (unsigned long long)1ll, (unsigned long long)-1ll,
    (unsigned long long)0x7fffffffffffffffll, (unsigned long long)(-0x7fffffffffffffffll - 1),
    (unsigned long long)0xfff0ll, (unsigned long long)0xffefll,
    (unsigned long long)0x100000000ll, (unsigned long long)-0x100000000ll,
    (unsigned long long)2049ll, (unsigned long long)0x0020000000000001ll,
    (unsigned long long)0x1010000000000001ll, (unsigned long long)0x00ff800000000001ll,
    (unsigned long long)-65520ll, (unsigned long long)65504ll};
static const HO to_other_r[] = {EACH(TO_OTHER)};
static const H from_other_r[] = {EACH(FROM_OTHER)};

static int hash_halves(const H *values, unsigned count, int line)
{
    for (unsigned i = 0; i < count; ++i) {
        r.h = values[i];
        MIX(r.u);
    }
    return finish(line);
}
#define HALVES(table, line) hash_halves(table, COUNT(table), line)
#else
static volatile H values[] = {EACH(V)};
static volatile HO other_values[] = {EACH(VO)};
static volatile float narrow32_in[] = {NARROW32};
static volatile double narrow64_in[] = {NARROW64};
static volatile H to_int_in[] = {TO_INT(HALF)};
static volatile int int32_in[] = {INT32};
static volatile long long int64_in[] = {INT64};
#endif

static int run(void)
{
    int bad = 0;
#if FOLD
    const H *const arithmetic[] = {add_r, sub_r, mul_r, div_r};
    for (int w = 0; w < 4; ++w)
        if (SELECTED(w) && (bad = hash_halves(arithmetic[w], 22 * 22, w)))
            return bad;
    if (SELECTED(4) && (bad = HALVES(neg_r, 4)))
        return bad;
    for (int w = 0; w < 6; ++w) {
        if (!SELECTED(5 + w))
            continue;
        for (unsigned i = 0; i < 22 * 22; ++i)
            MIX(compare_r[w][i]);
        if ((bad = finish(5 + w)))
            return bad;
    }
    if (SELECTED(11)) {
        for (unsigned i = 0; i < COUNT(widen32_r); ++i) {
            f.f = widen32_r[i];
            MIX(f.u);
        }
        if ((bad = finish(11)))
            return bad;
    }
    if (SELECTED(12)) {
        for (unsigned i = 0; i < COUNT(widen64_r); ++i) {
            d.d = widen64_r[i];
            MIX(d.u);
        }
        if ((bad = finish(12)))
            return bad;
    }
    if (SELECTED(13) && (bad = HALVES(narrow32_r, 13)))
        return bad;
    if (SELECTED(14) && (bad = HALVES(narrow64_r, 14)))
        return bad;
    if (SELECTED(15)) {
        for (unsigned i = 0; i < COUNT(to_int_r); ++i)
            MIX((unsigned)to_int_r[i]);
        if ((bad = finish(15)))
            return bad;
    }
    if (SELECTED(16) && (bad = HALVES(from_int_r, 16)))
        return bad;
    if (SELECTED(17) && (bad = HALVES(from_long_r, 17)))
        return bad;
    if (SELECTED(18) && (bad = HALVES(from_ulong_r, 18)))
        return bad;
    if (SELECTED(19)) {
        for (unsigned i = 0; i < COUNT(to_other_r); ++i) {
            o.h = to_other_r[i];
            MIX(o.u);
        }
        if ((bad = finish(19)))
            return bad;
    }
    if (SELECTED(20) && (bad = HALVES(from_other_r, 20)))
        return bad;
#else
    for (int w = 0; w < 4; ++w) {
        if (!SELECTED(w))
            continue;
        for (unsigned i = 0; i < 22; ++i)
            for (unsigned j = 0; j < 22; ++j) {
                H x = values[i], y = values[j];
                r.h = w == 0 ? ADD(x, y) : w == 1 ? SUB(x, y) : w == 2 ? MUL(x, y) : DIV(x, y);
                MIX(r.u);
            }
        if ((bad = finish(w)))
            return bad;
    }
    if (SELECTED(4)) {
        for (unsigned i = 0; i < 22; ++i) {
            r.h = -values[i];
            MIX(r.u);
        }
        if ((bad = finish(4)))
            return bad;
    }
    for (int w = 0; w < 6; ++w) {
        if (!SELECTED(5 + w))
            continue;
        for (unsigned i = 0; i < 22; ++i)
            for (unsigned j = 0; j < 22; ++j) {
                H x = values[i], y = values[j];
                int c = w == 0 ? EQ(x, y) : w == 1 ? NE(x, y) : w == 2 ? LT(x, y)
                      : w == 3 ? LE(x, y) : w == 4 ? GT(x, y) : GE(x, y);
                MIX(c);
            }
        if ((bad = finish(5 + w)))
            return bad;
    }
    if (SELECTED(11)) {
        for (unsigned i = 0; i < 22; ++i) {
            f.f = values[i];
            MIX(f.u);
        }
        if ((bad = finish(11)))
            return bad;
    }
    if (SELECTED(12)) {
        for (unsigned i = 0; i < 22; ++i) {
            d.d = values[i];
            MIX(d.u);
        }
        if ((bad = finish(12)))
            return bad;
    }
    if (SELECTED(13)) {
        for (unsigned i = 0; i < COUNT(narrow32_in); ++i) {
            r.h = (H)narrow32_in[i];
            MIX(r.u);
        }
        if ((bad = finish(13)))
            return bad;
    }
    if (SELECTED(14)) {
        for (unsigned i = 0; i < COUNT(narrow64_in); ++i) {
            r.h = (H)narrow64_in[i];
            MIX(r.u);
        }
        if ((bad = finish(14)))
            return bad;
    }
    if (SELECTED(15)) {
        for (unsigned i = 0; i < COUNT(to_int_in); ++i)
            MIX((unsigned)(int)to_int_in[i]);
        if ((bad = finish(15)))
            return bad;
    }
    if (SELECTED(16)) {
        for (unsigned i = 0; i < COUNT(int32_in); ++i) {
            r.h = (H)int32_in[i];
            MIX(r.u);
        }
        if ((bad = finish(16)))
            return bad;
    }
    if (SELECTED(17)) {
        for (unsigned i = 0; i < COUNT(int64_in); ++i) {
            r.h = (H)int64_in[i];
            MIX(r.u);
        }
        if ((bad = finish(17)))
            return bad;
    }
    if (SELECTED(18)) {
        for (unsigned i = 0; i < COUNT(int64_in); ++i) {
            r.h = (H)(unsigned long long)int64_in[i];
            MIX(r.u);
        }
        if ((bad = finish(18)))
            return bad;
    }
    if (SELECTED(19)) {
        for (unsigned i = 0; i < 22; ++i) {
            o.h = (HO)values[i];
            MIX(o.u);
        }
        if ((bad = finish(19)))
            return bad;
    }
    if (SELECTED(20)) {
        for (unsigned i = 0; i < 22; ++i) {
            r.h = (H)other_values[i];
            MIX(r.u);
        }
        if ((bad = finish(20)))
            return bad;
    }
#endif
    return bad;
}

int main(void)
{
    return run();
}
