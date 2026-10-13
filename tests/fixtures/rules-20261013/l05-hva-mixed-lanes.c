/* L05 probe: AAPCS64 homogeneity tests only the short-vector size, not
 * the lane type, so a record of two 16-byte vectors with different lane
 * types is an HVA passed and returned in v0 and v1.
 * Build twice (PART=1 callee, PART=2 caller) and link across compilers.
 * CASE=0: int x4 with short x8 (integer lanes only; current scope);
 * CASE=1: float x4 with int x4; CASE=2: float x2 with double x1 (8 bytes);
 * CASE=3: two float x4 members (an ordinary floating HVA).
 * Exit status 0 means the call kept every lane. */
#ifndef CASE
#define CASE 0
#endif
#ifndef PART
#define PART 2
#endif
#if CASE == 0
typedef int A __attribute__((vector_size(16)));
typedef short B __attribute__((vector_size(16)));
#elif CASE == 1
typedef float A __attribute__((vector_size(16)));
typedef int B __attribute__((vector_size(16)));
#elif CASE == 2
typedef float A __attribute__((vector_size(8)));
typedef double B __attribute__((vector_size(8)));
#else
typedef float A __attribute__((vector_size(16)));
typedef float B __attribute__((vector_size(16)));
#endif
struct mixed { A a; B b; };
struct mixed bump(struct mixed);
#if PART == 1
struct mixed bump(struct mixed m)
{
    m.a += 1;
    m.b += 2;
    return m;
}
#else
int main(void)
{
#if CASE == 2
    struct mixed m = {{1, 2}, {5}};
#else
    struct mixed m = {{1, 2}, {5, 6}};
#endif
    m = bump(m);
    return m.a[0] != 2 || m.a[1] != 3 || m.b[0] != 7;
}
#endif
