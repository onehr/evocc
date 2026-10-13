/* L05 probe: which floating-vector forms the current compiler already
 * executes without any lane arithmetic. Exit status 0 means success.
 *  0 brace initializer, runtime-index lane read and write
 *  1 whole-vector assignment from another object (v = w)
 *  2 vector load through a cast pointer: v = *(f4 *)p, then lane reads
 *  3 union of a vector with a float array (write array, read vector lane)
 *  4 lane reads through (float *)&v, as scal-to-vec1's vidx macro does
 *  5 compound literal value assigned to a vector object
 *  6 vector returned from a function and passed as an argument
 *  7 whole-vector store through a cast pointer: *(f4 *)p = v */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
static volatile int index1 = 1;
static float buffer[8] __attribute__((aligned(16))) = {1, 2, 3, 4, 5, 6, 7, 8};

#if CASE == 6
__attribute__((noinline)) static f4 pass(f4 v) { return v; }
#endif

int main(void)
{
    f4 v = {1.5f, -2, 3, 0};
#if CASE == 0
    float x = v[index1];
    v[index1 + 1] = x;
    return v[2] != -2;
#elif CASE == 1
    f4 w = v;
    return w[index1] != -2;
#elif CASE == 2
    f4 w = *(f4 *)(buffer + 4);
    return w[index1] != 6;
#elif CASE == 3
    union { f4 v; float a[4]; } u;
    for (int k = 0; k < 4; ++k)
        u.a[k] = (float)k;
    return u.v[index1] != 1;
#elif CASE == 4
    return *((float *)&v + index1) != -2;
#elif CASE == 5
    v = (f4){5, 6, 7, 8};
    return v[index1] != 6;
#elif CASE == 6
    f4 w = pass(v);
    return w[index1] != -2;
#elif CASE == 7
    *(f4 *)buffer = v;
    return buffer[index1] != -2;
#else
#error unsupported CASE
#endif
}
