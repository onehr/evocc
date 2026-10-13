/* L04 code-generation probe: compile with -S and read the calls.
 * One function per source form. Nothing here runs; the assembly is the data.
 * CC -std=gnu11 -O0|-O2 [-ffp-contract=off] [-fcx-limited-range] -S FILE */
typedef float _Complex cf;
typedef double _Complex cd;
typedef long double _Complex cq;

cf mul_f(cf a, cf b) { return a * b; }
cd mul_d(cd a, cd b) { return a * b; }
cq mul_q(cq a, cq b) { return a * b; }
cf div_f(cf a, cf b) { return a / b; }
cd div_d(cd a, cd b) { return a / b; }
cq div_q(cq a, cq b) { return a / b; }

/* Mixed real and complex operands (C11 6.3.1.8, Annex G.5). */
cd mul_d_real(cd a, double r) { return a * r; }
cd mul_real_d(double r, cd b) { return r * b; }
cd div_d_real(cd a, double r) { return a / r; }
cd div_real_d(double r, cd b) { return r / b; }
cd add_d_real(cd a, double r) { return a + r; }
cd sub_real_d(double r, cd b) { return r - b; }
cd mul_d_int(cd a, int i) { return a * i; }

/* Compound assignment through memory. */
void mul_assign(cd *p, cd b) { *p *= b; }
void div_assign(cd *p, cd b) { *p /= b; }

/* Comparisons, truth, conversions and the GNU operators. */
int eq_d(cd a, cd b) { return a == b; }
int ne_d(cd a, cd b) { return a != b; }
int truth_d(cd a) { return a ? 1 : 0; }
int not_d(cd a) { return !a; }
double to_real(cd a) { return a; }
_Bool to_bool(cd a) { return a; }
long to_long(cd a) { return a; }
cf narrow(cd a) { return a; }
cd conj_d(cd a) { return ~a; }
cd neg_d(cd a) { return -a; }
cd build_d(double x, double y) { return __builtin_complex(x, y); }
double real_part(cd a) { return __real__ a; }
double imag_part(cd a) { return __imag__ a; }
void set_imag(cd *p, double y) { __imag__ *p = y; }
