#include "floating-quad-abi.h"
int qa_caller(void) {
    long double exact=0x1.00000000000000000000000001p0L;
    if(qa_mixed(1,2.5f,3.25L,4.125,5)!=15.875L) return 1;
    if(qa_nine(1.L,2.L,3.L,4.L,5.L,6.L,7.L,8.L,exact)!=36.L+exact) return 2;
    struct Q2 two={exact,-exact}; two=qa_two(two);
    if(two.a!=-exact||two.b!=exact) return 3;
    struct Q3 three={{exact,2*exact,3*exact}};
    struct Q3 r=qa_three(1,2,3,4,5,6,three);
    if(r.a[0]!=3*exact+3||r.a[1]!=2*exact+7||r.a[2]!=exact+11) return 4;
    struct Q4 four={{exact,2*exact},{3*exact,4*exact}};
    four=qa_four(four);
    if(four.a.a!=3*exact||four.a.b!=4*exact||four.b.a!=exact||four.b.b!=2*exact) return 5;
    if(qa_stack(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,exact)!=153.L+exact) return 6;
    if(qa_variadic(9,1L,2.L,3L,4.L,5L,6.L,7L,8.L,9L,10.L,11L,12.L,13L,14.L,15L,16.L,17L,exact,three)!=153.L+7*exact) return 7;
    if(qa_pressure(exact,qa_clobber)!=153.L+18*exact) return 8;
    return 0;
}
