#include "integer-vector-varargs.h"
int main(void) {
    iv4 a={1,2,3,4};
    if(vector_variadic(a,9,a,10L,a,11L,a,12L,a,13L,a,14L,a,15L,a,16L,a,17L,a,18L)!=152) return 1;
    struct hv2 h={{2,3,4,5},{6,7,8,9}};
    if(vector_hva_variadic(5,h,h,h,h,h)!=55) return 2;
    return 0;
}
