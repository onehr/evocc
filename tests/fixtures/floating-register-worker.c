typedef unsigned int V __attribute__((vector_size(16)));
extern void floating_clobber(void);
static volatile double data[24] = {0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23};
static volatile V vectors[4] = {{1,2,3,4},{5,6,7,8},{9,10,11,12},{13,14,15,16}};
#define D(N) double d##N = data[N] + x;
#define F(N) float f##N = (float)data[N] + (float)x;
double floating_pressure(double x) {
    D(0) D(1) D(2) D(3) D(4) D(5) D(6) D(7)
    D(8) D(9) D(10) D(11) D(12) D(13) D(14) D(15)
    D(16) D(17) D(18) D(19) D(20) D(21) D(22) D(23)
    F(0) F(1) F(2) F(3) F(4) F(5) F(6) F(7)
    V a=vectors[0], b=vectors[1], c=vectors[2], d=vectors[3];
    floating_clobber();
    V sum = a+b+c+d;
    if (sum[0]!=28 || sum[1]!=32 || sum[2]!=36 || sum[3]!=40) return -1.0;
    return d0+d1+d2+d3+d4+d5+d6+d7+d8+d9+d10+d11+d12+d13+d14+d15+d16+d17+d18+d19+d20+d21+d22+d23
        + f0+f1+f2+f3+f4+f5+f6+f7;
}
