/* L05: small independent arithmetic/mask/transport/shuffle/conversion REDs.
 * No architecture intrinsic header, so header admission is not conflated
 * with executable lane operations. CASE=0..3. */
#ifndef CASE
#define CASE 0
#endif
#if CASE == 1
typedef double vector __attribute__((vector_size(16)));
#else
typedef float vector __attribute__((vector_size(16)));
#endif
typedef int integers __attribute__((vector_size(16)));
#if CASE == 0 || CASE == 1
__attribute__((noinline)) vector step(vector a, vector b) { return a * b + a; }
#endif
int main(void)
{
#if CASE == 1
    vector a = {1, -2}, b = {2, 3};
    vector result = step(a, b);
    __typeof__(a < b) mask = a < b;
    return result[0] != 3 || result[1] != -8 || mask[0] != -1 || mask[1] != -1;
#elif CASE == 0
    vector a = {1, -2, 3, -4}, b = {2, 3, 4, 5};
    vector result = step(a, b);
    __typeof__(a < b) mask = a < b;
    return result[0] != 3 || result[1] != -8 || result[2] != 15
        || result[3] != -24 || mask[0] != -1 || mask[3] != -1;
#elif CASE == 2
    vector a = {1, -2, 3, -4};
    vector result = __builtin_shufflevector(a, a, 3, 2, 1, 0);
    return result[0] != -4 || result[1] != 3 || result[2] != -2 || result[3] != 1;
#elif CASE == 3
    vector a = {1.5f, -2.5f, 3.5f, -4.5f};
    integers result = __builtin_convertvector(a, integers);
    return result[0] != 1 || result[1] != -2 || result[2] != 3 || result[3] != -4;
#else
#error unsupported CASE
#endif
}
