/* L03: special values of the half formats, bit for bit.
 *
 * KIND=0: _Float16; KIND=1: __fp16 (both binary16); KIND=2: __bf16. Every
 * operation is single: its result is stored into an object of the half
 * type, so it rounds once, however the compiler evaluates it. The binary
 * operations take every ordered pair (i, j) of the special-value table T:
 * signed zeros, subnormals, the least normal, the largest finite value,
 * infinities, quiet NaNs with payloads, signaling NaNs and ordinary values.
 * Conversions use their own tables: ties and the values beside them,
 * overflow thresholds, subnormal ties, the value that rounds to the least
 * normal, NaNs with payloads and integers of each width.
 *
 * Each operation gets one 64-bit hash of its result bits, in table order.
 * Under -DFLAGS, the hash also takes the sticky flags that each operation
 * raises; it needs -frounding-math, and MODE (0 nearest, 1 upward, 2
 * downward, 3 toward zero) selects the rounding direction. Without FLAGS,
 * MODE must be 0.
 *
 * Some inputs have lines of their own, because Apple clang computes them in
 * another way that IEEE 754 or C permits, or with a recorded defect. A pair
 * of NaN operands with a signaling one is the line nan_pairs: binary32
 * evaluation quiets both operands first, so the first operand's payload
 * wins, while native binary16 arithmetic prefers the signaling one. A
 * signaling NaN operand of negation or of a widening conversion is the
 * line neg_snan, widen32_snan or widen64_snan. The NaN inputs of the
 * binary64 narrowing are the line narrow64_nan.
 *
 * -DOTHER adds the conversions to and from the other half format, -DQUAD
 * the binary128 conversions (Linux long double) and -DWIDE the 128-bit
 * integer conversions. -DROW_FIRST=f and -DROW_STEP=s keep the rows f,
 * f + s, ... of the binary operations and the comparisons: every value
 * stays a right operand, and the slices of one step together take every
 * pair. -DPART=1 selects the arithmetic lines,
 * -DPART=2 the comparisons and -DPART=3 the conversions; in parts 1 and 2,
 * -DOP=n keeps operation n alone (0 is the first). The EvoCC interpreters
 * run one operation at a time, within their step budget.
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
#define KIND 0
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
#ifndef ROW_FIRST
#define ROW_FIRST 0
#endif
#ifndef ROW_STEP
#define ROW_STEP 1
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

#if BF
static const u16 table[] = {
    0x0000, 0x8000, 0x0001, 0x007f, 0x0080, 0x3f80, 0x3f81, 0x3fc0,
    0x4040, 0x3dcd, 0x7f7f, 0x7f80, 0xff80, 0x7fc0, 0x7fd1, 0xffe2,
    0x7f91, 0xffa2, 0xbfc0, 0x4380, 0x3b80, 0xc080, 0x0041, 0x7f00};
/* Values of T that convert to int with a defined result. */
static const u16 to_int_in[] = {
    0x0000, 0x8000, 0x3f00, 0xbf00, 0x3fc0, 0xbfc0, 0x4020, 0x4380,
    0x4eff, 0xcf00, 0x0001, 0x3f7f};
static const unsigned narrow32_in[] = {
    0x00000000u, 0x80000000u, 0x3f800000u, 0x3f808000u, 0x3f818000u,
    0x3f808001u, 0x3f807fffu, 0x7f7f0000u, 0x7f7f8000u, 0x7f7f7fffu,
    0x7f7fffffu, 0x00008000u, 0x00008001u, 0x00018000u, 0x007f8000u,
    0x007fc000u, 0x00800000u, 0x7f800000u, 0xff800000u, 0x7fc00000u,
    0xffc12345u, 0x7f800001u, 0x7fbfffffu, 0x00000001u, 0xff7fffffu,
    0x3dcccccdu, 0xbf808000u, 0x80018000u};
static const u64 narrow64_in[] = {
    0x0000000000000000ull, 0x8000000000000000ull, 0x3ff0000000000000ull,
    0x3ff0100000000000ull, 0x3ff0100000001000ull, 0x3ff0300000000000ull,
    0x3ff00fffffffffffull, 0x47efe00000000000ull, 0x47eff00000000000ull,
    0x47efefffffffffffull, 0x7fefffffffffffffull, 0x3800000000000000ull,
    0x3790000000000000ull, 0x3790000000000001ull, 0x37a8000000000000ull,
    0x380ff00000000000ull, 0x380ff7ffffffffffull, 0x7ff0000000000000ull,
    0xfff0000000000000ull, 0x0000000000000001ull, 0x3fb999999999999aull,
    0xbff0100000000001ull, 0x47f0000000000000ull};
static const u64 narrow64_nan_in[] = {
    0x7ff8000000000000ull, 0x7fffffffffffffffull, 0xfff8000000000123ull,
    0x7ff0000000000001ull, 0xfff7ffffffffffffull, 0x7ff4000000000000ull};
static const unsigned int32_in[] = {
    0, 1, 0xffffffffu, 257, 259, 0xfffffeffu, 0x7fffffffu, 0x80000000u,
    0x01010001u, 0x00ff8000u, 0x00ff7fffu, 0x7f7fffffu, 65520, 3};
static const u64 int64_in[] = {
    0, 1, 0xffffffffffffffffull, 0x7fffffffffffffffull,
    0x8000000000000000ull, 0x1010000000000001ull, 0x1010000000000000ull,
    0x0020000000000001ull, 0xffdfffffffffffffull, 0x00ff800000000000ull,
    0x00ff800000000001ull, 257, 0x0000000100000000ull};
#else
static const u16 table[] = {
    0x0000, 0x8000, 0x0001, 0x03ff, 0x0400, 0x3c00, 0x3c01, 0x3e00,
    0x4200, 0x2e66, 0x7bff, 0x7c00, 0xfc00, 0x7e00, 0x7e11, 0xfe22,
    0x7c11, 0xfd22, 0xbe00, 0x5c00, 0x1c00, 0xc400, 0x0201, 0x7800};
static const u16 to_int_in[] = {
    0x0000, 0x8000, 0x3800, 0xb800, 0x3e00, 0xbe00, 0x4100, 0x7bff,
    0xfbff, 0x0001, 0x3bff, 0x6800};
static const unsigned narrow32_in[] = {
    0x00000000u, 0x80000000u, 0x3f800000u, 0x3f801000u, 0x3f803000u,
    0x3f801001u, 0x3f800fffu, 0x477fe000u, 0x477ff000u, 0x477fefffu,
    0x47800000u, 0x33000000u, 0x33000001u, 0x33c00000u, 0x387fe000u,
    0x387ff000u, 0x38800000u, 0x7f800000u, 0xff800000u, 0x7fc00000u,
    0xffc12345u, 0x7f800001u, 0x7fbfffffu, 0x7f7fffffu, 0x00000001u,
    0xc7800000u, 0x3dcccccdu, 0xbf801000u};
static const u64 narrow64_in[] = {
    0x0000000000000000ull, 0x8000000000000000ull, 0x3ff0000000000000ull,
    0x3ff0020000000000ull, 0x3ff0020000001000ull, 0x3ff0060000000000ull,
    0x3ff001ffffffffffull, 0x40effc0000000000ull, 0x40effe0000000000ull,
    0x40effdffffffffffull, 0x7fefffffffffffffull, 0x3e60000000000000ull,
    0x3e60000000000001ull, 0x3e78000000000000ull, 0x3f0ffe0000000000ull,
    0x3f0ffdffffffffffull, 0x3f10000000000000ull, 0x7ff0000000000000ull,
    0xfff0000000000000ull, 0x0000000000000001ull, 0x3fb999999999999aull,
    0xbff0020000000001ull, 0x40f0000000000000ull};
static const u64 narrow64_nan_in[] = {
    0x7ff8000000000000ull, 0x7fffffffffffffffull, 0xfff8000000000123ull,
    0x7ff0000000000001ull, 0xfff7ffffffffffffull, 0x7ff4000000000000ull};
static const unsigned int32_in[] = {
    0, 1, 0xffffffffu, 2049, 2051, 0xfffff7ffu, 0x7fffffffu, 0x80000000u,
    65504, 65519, 65520, 65535, 0xffff0010u, 4097};
static const u64 int64_in[] = {
    0, 1, 0xffffffffffffffffull, 0x7fffffffffffffffull,
    0x8000000000000000ull, 0x000000000000fff0ull, 0x000000000000ffefull,
    0x0000000100000000ull, 0xffffffff00000000ull, 2049,
    0x0020000000000001ull, 0xffffffffffff0010ull, 65504};
#endif
#define COUNT(a) (sizeof (a) / sizeof (a)[0])

static u64 hash;
/* One multiply and one shift per value keeps the interpreters' step count
 * small; a changed bit still changes the hash. */
#define MIX(value) (hash = (hash ^ (u64)(value)) * 0x100000001b3ull, hash ^= hash >> 29)

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

#define IS_NAN(bits) (BF ? ((bits) & 0x7fff) > 0x7f80 : ((bits) & 0x7fff) > 0x7c00)
#define IS_SIGNALING(bits) (BF ? (((bits) & 0x7fc0) == 0x7f80 && ((bits) & 0x003f)) \
                               : (((bits) & 0x7e00) == 0x7c00 && ((bits) & 0x01ff)))
/* Two NaN operands, one of them signaling. */
#define NAN_PAIR(x, y) (IS_NAN(x) && IS_NAN(y) && (IS_SIGNALING(x) || IS_SIGNALING(y)))

#ifdef CHECK
static const u64 expect[] = {EXPECT};
#else
static const char *const names[] = {
    "add", "sub", "mul", "div", "nan_pairs", "neg", "neg_snan", "eq", "ne",
    "lt", "le", "gt", "ge", "widen32", "widen32_snan", "widen64",
    "widen64_snan", "narrow32", "narrow64", "narrow64_nan", "to_int",
    "from_int", "from_long", "from_ulong", "to_other", "from_other",
    "narrow128", "widen128", "from_int128", "from_uint128"};
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
    printf("KIND=%d MODE=%d %s %016llx\n", KIND, MODE, names[index], hash);
#endif
    ++produced;
    hash = 0xcbf29ce484222325ull;
    return 0;
}

/* The objects live outside the loops: an interpreter keeps every object
 * that it creates, so a loop allocates nothing it does not need. */
static HB a, b, r;
static volatile int sink;

__attribute__((noinline)) static H arith(int which, H x, H y)
{
    switch (which) {
    case 0: return x + y;
    case 1: return x - y;
    case 2: return x * y;
    default: return x / y;
    }
}

__attribute__((noinline)) static int compare(int which, H x, H y)
{
    switch (which) {
    case 0: return x == y;
    case 1: return x != y;
    case 2: return x < y;
    case 3: return x <= y;
    case 4: return x > y;
    default: return x >= y;
    }
}

static int arithmetic(void)
{
    int bad;
    for (int w = 0; w < 5; ++w) {
        if (!SELECTED(w))
            continue;
        for (int op = w == 4 ? 0 : w; op < (w == 4 ? 4 : w + 1); ++op)
            for (unsigned i = ROW_FIRST; i < COUNT(table); i += ROW_STEP)
                for (unsigned j = 0; j < COUNT(table); ++j) {
                    if (NAN_PAIR(table[i], table[j]) != (w == 4))
                        continue;
                    a.u = table[i];
                    b.u = table[j];
                    BEGIN();
                    r.h = arith(op, a.h, b.h);
                    END();
                    MIX(r.u);
                }
        if ((bad = finish(w)))
            return bad;
    }
    for (int snan = 0; snan < 2; ++snan) {
        if (!SELECTED(5 + snan))
            continue;
        for (unsigned i = 0; i < COUNT(table); ++i) {
            if (IS_SIGNALING(table[i]) != snan)
                continue;
            a.u = table[i];
            BEGIN();
            r.h = -a.h;
            END();
            MIX(r.u);
        }
        if ((bad = finish(5 + snan)))
            return bad;
    }
    return 0;
}

static int comparisons(void)
{
    int bad;
    for (int w = 0; w < 6; ++w) {
        if (!SELECTED(w))
            continue;
        for (unsigned i = ROW_FIRST; i < COUNT(table); i += ROW_STEP)
            for (unsigned j = 0; j < COUNT(table); ++j) {
                a.u = table[i];
                b.u = table[j];
                BEGIN();
                sink = compare(w, a.h, b.h);
                END();
                MIX(sink);
            }
        if ((bad = finish(7 + w)))
            return bad;
    }
    return 0;
}

static FB f;
static DB d;
static OB o;

static int conversions(void)
{
    int bad;
    for (int snan = 0; snan < 2; ++snan) {
        for (unsigned i = 0; i < COUNT(table); ++i) {
            if (IS_SIGNALING(table[i]) != snan)
                continue;
            a.u = table[i];
            BEGIN();
            f.f = a.h;
            END();
            MIX(f.u);
        }
        if ((bad = finish(13 + snan)))
            return bad;
    }
    for (int snan = 0; snan < 2; ++snan) {
        for (unsigned i = 0; i < COUNT(table); ++i) {
            if (IS_SIGNALING(table[i]) != snan)
                continue;
            a.u = table[i];
            BEGIN();
            d.d = a.h;
            END();
            MIX(d.u);
        }
        if ((bad = finish(15 + snan)))
            return bad;
    }
    for (unsigned i = 0; i < COUNT(narrow32_in); ++i) {
        f.u = narrow32_in[i];
        BEGIN();
        r.h = (H)f.f;
        END();
        MIX(r.u);
    }
    if ((bad = finish(17)))
        return bad;
    for (unsigned i = 0; i < COUNT(narrow64_in); ++i) {
        d.u = narrow64_in[i];
        BEGIN();
        r.h = (H)d.d;
        END();
        MIX(r.u);
    }
    if ((bad = finish(18)))
        return bad;
    for (unsigned i = 0; i < COUNT(narrow64_nan_in); ++i) {
        d.u = narrow64_nan_in[i];
        BEGIN();
        r.h = (H)d.d;
        END();
        MIX(r.u);
    }
    if ((bad = finish(19)))
        return bad;
    for (unsigned i = 0; i < COUNT(to_int_in); ++i) {
        a.u = to_int_in[i];
        BEGIN();
        sink = (int)a.h;
        END();
        MIX((unsigned)sink);
    }
    if ((bad = finish(20)))
        return bad;
    for (unsigned i = 0; i < COUNT(int32_in); ++i) {
        volatile int in = (int)int32_in[i];
        BEGIN();
        r.h = (H)in;
        END();
        MIX(r.u);
    }
    if ((bad = finish(21)))
        return bad;
    for (unsigned i = 0; i < COUNT(int64_in); ++i) {
        volatile long long in = (long long)int64_in[i];
        BEGIN();
        r.h = (H)in;
        END();
        MIX(r.u);
    }
    if ((bad = finish(22)))
        return bad;
    for (unsigned i = 0; i < COUNT(int64_in); ++i) {
        volatile unsigned long long in = int64_in[i];
        BEGIN();
        r.h = (H)in;
        END();
        MIX(r.u);
    }
    if ((bad = finish(23)))
        return bad;
#ifdef OTHER
    for (unsigned i = 0; i < COUNT(table); ++i) {
        a.u = table[i];
        BEGIN();
        o.h = (HO)a.h;
        END();
        MIX(o.u);
    }
    if ((bad = finish(24)))
        return bad;
    for (unsigned i = 0; i < COUNT(table); ++i) {
        o.u = table[i];
        BEGIN();
        r.h = (H)o.h;
        END();
        MIX(r.u);
    }
    if ((bad = finish(25)))
        return bad;
#endif
#ifdef QUAD
    {
        static const long double quad_in[] = {
            0.0L, -0.0L, 1.0L, 1.0L + 0x1p-11L + 0x1p-100L, 1.0L + 0x1p-8L + 0x1p-100L,
            1.0L + 0x1p-11L, 1.0L + 0x1p-8L, 65504.0L, 65520.0L, 65519.999L,
            0x1.fep127L, 0x1.ff8p127L, 0x1p-24L, 0x1p-25L, 0x1.8p-24L, 0x1p-133L,
            0x1p-134L, 1e4000L, -1e4000L, 0.1L, 0x1p-16400L};
        for (unsigned i = 0; i < COUNT(quad_in); ++i) {
            volatile long double in = quad_in[i];
            BEGIN();
            r.h = (H)in;
            END();
            MIX(r.u);
        }
        if ((bad = finish(26)))
            return bad;
        for (unsigned i = 0; i < COUNT(table); ++i) {
            union { long double q; u64 u[2]; } wide;
            a.u = table[i];
            BEGIN();
            wide.q = a.h;
            END();
            MIX(wide.u[0]);
            MIX(wide.u[1]);
        }
        if ((bad = finish(27)))
            return bad;
    }
#endif
#ifdef WIDE
    {
        static const unsigned __int128 wide_in[] = {
            0, 1, ~(unsigned __int128)0, (unsigned __int128)1 << 127,
            ((unsigned __int128)1 << 100) + ((unsigned __int128)1 << 92) + 1,
            ((unsigned __int128)1 << 100) + ((unsigned __int128)1 << 92),
            65520, 65519, ((unsigned __int128)1 << 64) - 1,
            ~(unsigned __int128)0 - 65535};
        for (unsigned i = 0; i < COUNT(wide_in); ++i) {
            volatile __int128 in = (__int128)wide_in[i];
            BEGIN();
            r.h = (H)in;
            END();
            MIX(r.u);
        }
        if ((bad = finish(28)))
            return bad;
        for (unsigned i = 0; i < COUNT(wide_in); ++i) {
            volatile unsigned __int128 in = wide_in[i];
            BEGIN();
            r.h = (H)in;
            END();
            MIX(r.u);
        }
        if ((bad = finish(29)))
            return bad;
    }
#endif
    return 0;
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
