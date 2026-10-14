/* Variadic calls across the compiler boundary on arm64 macOS (D05). The
   cases follow spec/evomir.md Section 6.12. callee.c defines the variadic
   functions and the va_list readers; caller.c calls them and checks each
   result. One compiler builds callee.c and the other builds caller.c, in
   both directions. */

#include <stdarg.h>
#include <stddef.h>

typedef int v2i __attribute__((vector_size(8)));
typedef int v4i __attribute__((vector_size(16)));

struct r3 { char c[3]; };
struct r12 { int a, b, c; };
struct r16 { long a, b; };
struct a16 { _Alignas(16) long a; long b; };
struct big { long a[5]; };
struct h3 { float a, b, c; };
struct h4d { double a, b, c, d; };
struct hv2 { v4i a, b; };
struct hv1 { v2i a; };

double v_doubles(int n, ...);
long v_mixed(int n, ...);
long v_int128(int n, ...);
long double v_long_double(int n, ...);
double v_hfa(int n, ...);
long v_hva(int n, ...);
long v_records(int n, ...);
long v_copy(int n, ...);
int v_format(char *buffer, size_t size, const char *format, ...);
long v_after_stack(long a0, long a1, long a2, long a3, long a4, long a5, long a6,
                   long a7, char c, int i, ...);
long sum_list(int n, va_list list);
long forward(int n, ...);
