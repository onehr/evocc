/* Shared declarations of the floating-complex crossed ABI probe (L04).
 * One compiler builds floating-complex-abi-worker.c and another builds
 * floating-complex-abi-driver.c; the driver calls every worker function.
 * Each complex type appears as an argument, a result, a variadic argument
 * and a record member. Every value is exact, so a register or stack
 * mismatch shows as a wrong result. */
#include <stdarg.h>
typedef float _Complex cf;
typedef double _Complex cd;
typedef long double _Complex cq;
struct tagged { char tag; cd z; };   /* not an HFA: general registers or memory */
struct hfa3 { cd z; double w; };     /* HFA of three doubles */
struct hq { cq z; };                 /* HFA of two long doubles */
struct hf { cf a; cf b; };           /* HFA of four floats */
struct mixed { cf a; float b; int c; }; /* not an HFA */

cf w_f(cf a, cf b);
cd w_d(cd a, double x, cd b);
cq w_q(cq a, cq b);
cd w_many(cd a0, cd a1, cd a2, cd a3, cd a4);
cd w_mixed(float f, cd a, cf b, cq c, double d);
cf w_spill(double r0, double r1, double r2, double r3, double r4, double r5,
           double r6, cq c, cf g);
double w_va(int n, ...);
cd w_struct(struct tagged t, struct hfa3 h, struct hq q);
struct hfa3 w_ret3(cd z);
struct hq w_retq(cq z);
struct hf w_retf(cf a, cf b);
cf w_records(struct hf h, struct mixed m);
cd w_cb(cd (*f)(cd, cd), cd a, cd b);
cq w_va_q(int n, ...);
