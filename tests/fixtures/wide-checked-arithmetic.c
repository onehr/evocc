typedef unsigned __int128 U;
typedef __int128 I;

__attribute__((noinline)) int add_u(U a, U b, U *p) { return __builtin_add_overflow(a, b, p); }
__attribute__((noinline)) int sub_u(U a, U b, U *p) { return __builtin_sub_overflow(a, b, p); }
__attribute__((noinline)) int mul_u(U a, U b, U *p) { return __builtin_mul_overflow(a, b, p); }
__attribute__((noinline)) int add_i(I a, I b, I *p) { return __builtin_add_overflow(a, b, p); }
__attribute__((noinline)) int sub_i(I a, I b, I *p) { return __builtin_sub_overflow(a, b, p); }
__attribute__((noinline)) int mul_i(I a, I b, I *p) { return __builtin_mul_overflow(a, b, p); }
__attribute__((noinline)) int add_mixed(I a, U b, U *p) { return __builtin_add_overflow(a, b, p); }
__attribute__((noinline)) int mul_mixed(I a, U b, I *p) { return __builtin_mul_overflow(a, b, p); }
__attribute__((noinline)) int narrow(U a, I b, signed char *p) { return __builtin_add_overflow(a, b, p); }

int main(void) {
    U umax = ~(U)0, u = 0;
    I imax = (I)(umax >> 1), imin = -imax - 1, i = 0;
    if (add_u(umax, 1, &u) != 1 || u != 0) return 1;
    if (add_u(umax, 0, &u) != 0 || u != umax) return 2;
    if (sub_u(0, 1, &u) != 1 || u != umax) return 3;
    if (sub_u(umax, umax, &u) != 0 || u != 0) return 4;
    if (mul_u(umax, umax, &u) != 1 || u != 1) return 5;
    if (mul_u((U)1 << 100, (U)1 << 20, &u) != 0 || u != (U)1 << 120) return 6;
    if (mul_u((U)1 << 100, (U)1 << 40, &u) != 1 || u != 0) return 7;
    if (add_i(imax, 1, &i) != 1 || i != imin) return 8;
    if (sub_i(imin, 1, &i) != 1 || i != imax) return 9;
    if (mul_i(imin, -1, &i) != 1 || i != imin) return 10;
    if (mul_i(imin, 1, &i) != 0 || i != imin) return 11;
    if (mul_i(imin, 0, &i) != 0 || i != 0) return 12;
    if (add_mixed(-1, umax, &u) != 0 || u != umax - 1) return 13;
    if (add_mixed(-1, 0, &u) != 1 || u != umax) return 14;
    if (mul_mixed(-1, umax, &i) != 1 || i != 1) return 15;
    if (mul_mixed(-3, 7, &i) != 0 || i != -21) return 16;
    signed char small = 0;
    if (narrow((U)1 << 100, -5, &small) != 1 || small != -5) return 17;
    if (narrow(4, -5, &small) != 0 || small != -1) return 18;
    U left[1] = {umax}, right[1] = {1}, output[1] = {99};
    int l = 0, r = 0, p = 0;
    if (__builtin_add_overflow(left[l++], right[r++], &output[p++]) != 1) return 19;
    if (l != 1 || r != 1 || p != 1 || output[0] != 0) return 20;
    return 0;
}
