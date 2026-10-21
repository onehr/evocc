/* L05: the forms that follow GCC 14 (`++`, `--` and `&v[i]` on floating
 * vectors), a conditional of vector values, a static vector, a wide
 * conversion, a shuffle with an unspecified lane and the mask lane types:
 * `long` on Linux, as in GCC 14, and `long long` on Darwin, as in Apple
 * clang. Exit 0 means every check passed. Apple clang refuses `v++`. */
typedef float f4 __attribute__((vector_size(16)));
typedef float f8 __attribute__((vector_size(32)));
typedef double d2 __attribute__((vector_size(16)));
typedef double d8 __attribute__((vector_size(64)));
typedef int i4 __attribute__((vector_size(16)));
static volatile int one = 1;
#ifdef __APPLE__
typedef long long mask64;
#else
typedef long mask64;
#endif
_Static_assert(__builtin_types_compatible_p(__typeof__(((d2){0} < (d2){0})[0]), mask64),
               "the mask lane type of the target's reference compiler");
static f4 global = {1.5f, -2, 3, 0.25f};
static d2 doubles = {0.5, -0.0};

int main(void)
{
    f4 v = global;
    f4 old = v++;
    if (old[0] != 1.5f || v[0] != 2.5f || v[3] != 1.25f) return 1;
    --v;
    if (v[1] != -2) return 2;
    f4 pre = ++v;
    if (pre[2] != 4 || v[2] != 4) return 3;
    v--;
    float *p = &v[1];
    *p = 7;
    if (v[1] != 7 || p[1] != 3) return 4;
    f8 w = {1, 2, 3, 4, 5, 6, 7, 8};
    f8 w_old = w++;
    if (w_old[7] != 8 || w[7] != 9 || w[0] != 2) return 5;
    float *q = &w[6];
    if (*q != 8) return 6;
    d8 big = __builtin_convertvector(w, d8);
    if (big[7] != 9) return 7;
    f4 a = {1, 2, 3, 4}, b = {5, 6, 7, 8};
    f4 c = one ? a : b;
    if (c[2] != 3) return 8;
    f4 s = __builtin_shufflevector(a, b, 7, -1, 0, -1);
    if (s[0] != 8 || s[2] != 1) return 9;
    d2 n = -doubles;
    union { d2 v; unsigned long long u[2]; } bits = {n};
    if (bits.u[1] != 0) return 10;
    i4 m = a < b;
    if (m[0] != -1) return 11;
    return 0;
}
