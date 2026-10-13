/* L05 probe: floating vectors under -mgeneral-regs-only (compile only).
 * CASE=0 float x4 parameter and result; CASE=1 local float x4
 * arithmetic on a runtime value with an integer result; CASE=2 a float x4
 * object that is only copied; CASE=3 control: int x4 parameter and
 * result; CASE=4 control: scalar float arithmetic on a runtime value. */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef int i4 __attribute__((vector_size(16)));
#if CASE == 0
f4 add(f4 a, f4 b) { return a + b; }
#elif CASE == 1
int lanes(int x)
{
    f4 a = {1, 2, 3, 4};
    a[0] = x;
    a = a * 2;
    return (int)a[0];
}
#elif CASE == 2
f4 g, h;
void copy(void) { g = h; }
#elif CASE == 3
i4 add(i4 a, i4 b) { return a + b; }
#elif CASE == 4
int scale(int x)
{
    float a = x;
    return (int)(a * 2.5f);
}
#else
#error unsupported CASE
#endif
