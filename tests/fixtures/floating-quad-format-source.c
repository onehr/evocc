/* Subject side of the crossed format-conversion matrix. */
#pragma STDC FENV_ACCESS ON
__attribute__((noinline)) float quad_narrow32(long double x) { return x; }
__attribute__((noinline)) double quad_narrow64(long double x) { return x; }
__attribute__((noinline)) long double quad_widen32(float x) { return x; }
__attribute__((noinline)) long double quad_widen64(double x) { return x; }
