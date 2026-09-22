#include "floating-abi.h"

double fp_mix(int a, float b, double c, long d) {
    return a + b + c + d;
}
double fp_stack(double a, double b, double c, double d, double e, double f,
                double g, double h, double i, double j, double k, double l) {
    return a + b*2 + c*3 + d*4 + e*5 + f*6 + g*7 + h*8
        + i*9 + j*10 + k*11 + l*12;
}
double fp_variadic(int n, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    double sum = 0;
    for (int i = 0; i < n; ++i) sum += __builtin_va_arg(ap, double);
    __builtin_va_end(ap);
    return sum;
}
struct floats4 fp_hfa(struct floats4 x, struct doubles2 y) {
    x.a += y.a; x.d += y.b;
    return x;
}
double fp_callback(double (*f)(int, float, double, long)) {
    return f(3, 1.25f, 2.5, 4);
}
double fp_hfa_stack(double a, double b, double c, double d, double e, double f,
                    double g, struct floats3 h, long i, double j) {
    return a+b+c+d+e+f+g+h.a+2*h.b+3*h.c+i+j;
}
double fp_hfa_variadic(int n, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    double sum = 0;
    for (int i = 0; i < n; ++i) {
        struct floats3 x = __builtin_va_arg(ap, struct floats3);
        sum += x.a + 2*x.b + 3*x.c;
    }
    sum += __builtin_va_arg(ap, double);
    __builtin_va_end(ap);
    return sum;
}
