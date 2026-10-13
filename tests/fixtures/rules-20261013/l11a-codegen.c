/* L11a: code-shape probe for _Atomic operators; compile with -S only. */
struct pair { int a, b; };
_Atomic unsigned x;
int *_Atomic p;
_Atomic double d;
_Atomic long double q;
_Atomic struct pair s;
_Atomic _Bool b;

unsigned read_x(void) { return x; }
void write_x(unsigned v) { x = v; }
unsigned add_x(unsigned v) { return x += v; }
unsigned post_x(void) { return x++; }
unsigned mul_x(unsigned v) { return x *= v; }
unsigned shl_x(unsigned v) { return x <<= v; }
int *step_p(void) { return p += 2; }
double add_d(double v) { return d += v; }
long double add_q(long double v) { return q += v; }
struct pair read_s(void) { return s; }
void write_s(struct pair v) { s = v; }
_Bool inc_b(void) { return b++; }
