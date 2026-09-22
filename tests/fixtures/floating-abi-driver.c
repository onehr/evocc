#include "floating-abi.h"
int main(void) {
    if (fp_mix(3, 1.25f, 2.5, 4) != 10.75) return 1;
    if (fp_stack(1,2,3,4,5,6,7,8,9,10,11,12) != 650) return 2;
    if (fp_variadic(12, 1.0f, 2.0, 3.0, 4.0, 5.0, 6.0,
                    7.0, 8.0, 9.0, 10.0, 11.0, 12.0) != 78) return 3;
    struct floats4 x = {1,2,3,4};
    struct doubles2 y = {10,20};
    x = fp_hfa(x, y);
    if (x.a != 11 || x.b != 2 || x.c != 3 || x.d != 24) return 4;
    if (fp_callback(fp_mix) != 10.75) return 5;
    struct floats3 z = {2,3,4};
    if (fp_hfa_stack(1,2,3,4,5,6,7,z,8,9) != 65) return 6;
    return fp_hfa_variadic(4,z,z,z,z,7.0) != 87;
}
