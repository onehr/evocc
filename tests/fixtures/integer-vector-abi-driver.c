#include "integer-vector-abi.h"
int vector_callback(int n) { return n + 3; }
int main(void) {
    i4 a = {1, 2, 3, 4}, b = {5, 6, 7, 8};
    i4 c = vector_add(a, b);
    if (c[0] != 6 || c[3] != 12) return 1;
    s4 small = vector_small((s4){1,2,3,4}, (s4){5,6,7,8});
    if (small[0] != 6 || small[3] != 12) return 2;
    if (vector_ninth(10,a,a,a,a,a,a,a,a,b,20) != 57) return 3;
    struct hva h = {a, b, a};
    h = vector_hva(h, b);
    if (h.x[1] != 8 || h.y[2] != 14 || h.z[3] != 12) return 4;
    i8 w = vector_wide((i8){0,1,2,3,4,5,6,7});
    if (w[0] != 3 || w[7] != 10) return 5;
    c = vector_preserved(a);
    if (c[0] != 13 || c[3] != 16) return 6;
    s4 v={1,2,3,4};
    if(vector_mixed_stack(1,2,3,4,5,6,7,8,v,v,v,v,v,v,v,v,v,((__int128)13<<64)+11)!=69)return 7;
    h=vector_hva_stack(a,a,a,a,a,a,(struct hva){a,b,a},7);
    if(h.x[3]!=16||h.y[3]!=20||h.z[3]!=11)return 8;
    return 0;
}
