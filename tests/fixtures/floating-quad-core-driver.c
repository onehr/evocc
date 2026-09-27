#include <stdio.h>
extern int quad_core(void), quad_nan(void), quad_dynamic(void), quad_varargs(void);
int main(void) {
    int r = quad_core(); if (r) {printf("quad_core %d\n", r); return 1;}
    r = quad_nan(); if (r) {printf("quad_nan %d\n", r); return 2;}
    r = quad_dynamic(); if (r) {printf("quad_dynamic %d\n", r); return 3;}
    r = quad_varargs(); if (r) {printf("quad_varargs %d\n", r); return 4;}
    puts("quad core: 4 programs PASS");
    return 0;
}
