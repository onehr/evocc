struct __attribute__((aligned(16))) H { float a, b; };
#ifndef DRIVER
struct H hfa_aligned(struct H x) { x.a += 1.0f; x.b += 2.0f; return x; }
float hfa_aligned_stack(double a, double b, double c, double d, double e, double f, double g, double h, struct H x, long tag) {
    return x.a + x.b + (float)tag + (float)(a+b+c+d+e+f+g+h);
}
#else
extern struct H hfa_aligned(struct H);
extern float hfa_aligned_stack(double,double,double,double,double,double,double,double,struct H,long);
#endif
#ifndef WORKER
int main(void) {
    struct H x = { 3.0f, 4.0f };
    struct H y = hfa_aligned(x);
    if (y.a != 4.0f || y.b != 6.0f) return 1;
    return hfa_aligned_stack(1,2,3,4,5,6,7,8,x,9) != 52.0f;
}
#endif
