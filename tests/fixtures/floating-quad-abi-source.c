#include "floating-quad-abi.h"
long double qa_mixed(int a,float b,long double c,double d,long e) { return a+b+c+d+e; }
long double qa_nine(long double a,long double b,long double c,long double d,long double e,long double f,long double g,long double h,long double i) { return a+b+c+d+e+f+g+h+i; }
struct Q2 qa_two(struct Q2 x) { struct Q2 y={x.b,x.a}; return y; }
struct Q3 qa_three(long double a,long double b,long double c,long double d,long double e,long double f,struct Q3 x) {
    struct Q3 y={{x.a[2]+a+b,x.a[1]+c+d,x.a[0]+e+f}}; return y;
}
struct Q4 qa_four(struct Q4 x) { struct Q4 y={x.b,x.a}; return y; }
long double qa_stack(long a,long double b,long c,long double d,long e,long double f,long g,long double h,long i,long double j,long k,long double l,long m,long double n,long o,long double p,long q,long double r) { return a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r; }
long double qa_variadic(int n,...) {
    __builtin_va_list ap; __builtin_va_start(ap,n);
    long double r=0;
    for(int i=0;i<n;++i) { r+=__builtin_va_arg(ap,long); r+=__builtin_va_arg(ap,long double); }
    struct Q3 x=__builtin_va_arg(ap,struct Q3);
    __builtin_va_end(ap); return r+x.a[0]+x.a[1]+x.a[2];
}
long double qa_pressure(long double a,void (*callback)(void)) {
    long double x0=a+0, x1=a+1, x2=a+2, x3=a+3, x4=a+4, x5=a+5,
        x6=a+6, x7=a+7, x8=a+8, x9=a+9, x10=a+10, x11=a+11,
        x12=a+12, x13=a+13, x14=a+14, x15=a+15, x16=a+16, x17=a+17;
    callback();
    return x0+x1+x2+x3+x4+x5+x6+x7+x8+x9+x10+x11+x12+x13+x14+x15+x16+x17;
}
