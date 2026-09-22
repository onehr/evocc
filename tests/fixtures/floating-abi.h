#ifndef EVOCC_FLOATING_ABI_H
#define EVOCC_FLOATING_ABI_H
struct floats4 { float a, b, c, d; };
struct doubles2 { double a, b; };
struct floats3 { float a, b, c; };
double fp_mix(int, float, double, long);
double fp_stack(double, double, double, double, double, double,
                double, double, double, double, double, double);
double fp_variadic(int, ...);
struct floats4 fp_hfa(struct floats4, struct doubles2);
double fp_callback(double (*)(int, float, double, long));
double fp_hfa_stack(double, double, double, double, double, double, double,
                    struct floats3, long, double);
double fp_hfa_variadic(int, ...);
#endif
