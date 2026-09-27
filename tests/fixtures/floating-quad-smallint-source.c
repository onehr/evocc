#pragma STDC FENV_ACCESS ON
__attribute__((noinline)) long double quad_from_i32(int x) { return x; }
__attribute__((noinline)) long double quad_from_u32(unsigned x) { return x; }
__attribute__((noinline)) long double quad_from_i64(long x) { return x; }
__attribute__((noinline)) long double quad_from_u64(unsigned long x) { return x; }
__attribute__((noinline)) int quad_to_i32(long double x) { return x; }
__attribute__((noinline)) unsigned quad_to_u32(long double x) { return x; }
__attribute__((noinline)) long quad_to_i64(long double x) { return x; }
__attribute__((noinline)) unsigned long quad_to_u64(long double x) { return x; }
