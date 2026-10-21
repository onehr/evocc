/* Shared shapes of the half-format crossed ABI probe (L03). KIND selects
 * the half type: 0 _Float16, 1 __fp16, 2 __bf16. -DLINUX adds the shapes
 * that AAPCS64 places on the stack and the anonymous half arguments, which
 * have no Apple placement in this revision: a half on the stack and an
 * anonymous _Float16 or __bf16. It also adds records that mix _Float16 and
 * __bf16 members: AAPCS64 gives the two formats two fundamental types, so
 * such a record is no HFA and passes in general registers, as GCC 14 passes
 * it. Clang 19 and Apple clang pass it in vector registers. A __bf16 record
 * is never an HFA here: GCC 14 and AAPCS64 disagree on it (GCC PR 120007). */
#ifndef FLOATING_HALF_ABI_H
#define FLOATING_HALF_ABI_H
#ifndef KIND
#define KIND 0
#endif
#if KIND == 0
typedef _Float16 H;
#elif KIND == 1
typedef __fp16 H;
#elif KIND == 2
typedef __bf16 H;
#else
#error unsupported KIND
#endif

typedef unsigned short u16;

/* An HFA of one to four binary16 members; for __bf16 a general record. */
struct H1 { H a; };
struct H2 { H a, b; };
struct H3 { H a, b, c; };
struct H4 { H a[4]; };
/* Not HFAs: five members, and mixed members. */
struct H5 { H a, b, c, d, e; };
struct HF { H a; float f; };
struct HI { H a; int i; H b; };

static inline u16 bits(H value)
{
    union { H h; u16 u; } v;
    v.h = value;
    return v.u;
}

static inline H make(u16 in)
{
    union { H h; u16 u; } v;
    v.u = in;
    return v.h;
}

#ifdef LINUX
struct HM { _Float16 a; __bf16 b; };
struct HN { __bf16 a; _Float16 b; __bf16 c; _Float16 d; };

static inline u16 f16_bits(_Float16 value)
{
    union { _Float16 h; u16 u; } v;
    v.h = value;
    return v.u;
}

static inline _Float16 f16_make(u16 in)
{
    union { _Float16 h; u16 u; } v;
    v.u = in;
    return v.h;
}

static inline u16 bf16_bits(__bf16 value)
{
    union { __bf16 h; u16 u; } v;
    v.h = value;
    return v.u;
}

static inline __bf16 bf16_make(u16 in)
{
    union { __bf16 h; u16 u; } v;
    v.u = in;
    return v.h;
}
#endif

H w_second(H a, H b);
unsigned w_mixed(float f0, H a, double d1, H b, int i, H c);
H w_eight(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7);
#if KIND != 2
struct H1 w_h1(struct H1 in);
struct H2 w_h2(struct H2 in);
struct H3 w_h3(struct H3 in);
struct H4 w_h4(struct H4 in, H extra);
#endif
struct H5 w_h5(struct H5 in);
struct HF w_hf(struct HF in);
struct HI w_hi(struct HI in);
double w_va_promoted(int n, ...);
#ifdef LINUX
struct HM w_hm(struct HM in);
struct HN w_hn(struct HN in, struct HM extra);
unsigned w_stack(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7, H a8, H a9);
#if KIND != 1
unsigned w_va_half(int n, ...);
#endif
#endif
#endif
