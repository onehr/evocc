/* Shared declarations of the floating-vector crossed ABI probe (L05).
 * One compiler builds floating-vector-abi-worker.c and another builds
 * floating-vector-abi-driver.c; the driver calls every worker function.
 * Each float and double vector of 8 and 16 bytes appears as an argument,
 * a result, a variadic argument and an HVA record member. AAPCS64 tests
 * only the short-vector size for homogeneity, so a record of vectors of
 * one size is an HVA whatever its lane types. Every value is exact, so a
 * register or stack mismatch shows as a wrong result. */
#ifndef FLOATING_VECTOR_ABI_H
#define FLOATING_VECTOR_ABI_H
#include <stdarg.h>
typedef float f2 __attribute__((vector_size(8)));
typedef float f4 __attribute__((vector_size(16)));
typedef double d1 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef int i2 __attribute__((vector_size(8)));
typedef int i4 __attribute__((vector_size(16)));
struct h2 { f2 a; d1 b; };            /* HVA of two 8-byte vectors */
struct h3 { f4 a; d2 b; i4 c; };      /* HVA of three 16-byte vectors */
struct h4 { d1 a[4]; };               /* HVA of four 8-byte vectors */
struct h5 { f2 a[5]; };               /* five members: not an HVA, 40 bytes */
struct hd { d2 a, b, c, d; };         /* HVA of four 16-byte vectors */
struct mixed { f2 a; f4 b; };         /* two sizes: not an HVA */
union lanes { f4 f; d2 d; i4 i; };    /* union HVA of one 16-byte vector */
struct padded { f2 a; } __attribute__((aligned(16))); /* padding: not an HVA */

f2 w_f2(f2 a, f2 b);
f4 w_f4(f4 a, float s, f4 b);
d1 w_d1(d1 a, double s, d1 b);
d2 w_d2(d2 a, d2 b);
f4 w_mix(float x, d1 a, double y, f2 b, d2 c, f4 d);
d2 w_stack(f4 a0, f4 a1, f4 a2, f4 a3, f4 a4, f4 a5, f4 a6, d1 a7, d2 s0,
           f2 s1);
double w_va(int n, ...);
struct h2 w_h2(struct h2 h, f2 v);
struct h3 w_h3(struct h3 h);
struct h4 w_h4(struct h4 h, d1 v);
struct h5 w_h5(struct h5 h);
struct hd w_hd(struct hd h);
struct mixed w_mixed(struct mixed m);
union lanes w_union(union lanes u);
struct padded w_padded(struct padded p, f2 v);
d2 w_spill(f4 a0, f4 a1, f4 a2, f4 a3, f4 a4, f4 a5, struct h3 h, d1 late);
double w_va_h2(int n, ...);
double w_va_h3(int n, ...);
double w_va_hd(int n, ...);
double w_va_union(int n, ...);
double w_va_mixed(int n, ...);
f4 w_callback(f4 (*f)(f4, d2), f4 a, d2 b);
#endif
