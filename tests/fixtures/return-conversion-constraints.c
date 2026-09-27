/* Return conversion uses the same admitted assignments as other value sites. */
#ifndef CASE
#define CASE 0
#endif

#if CASE == 1
int *probe(long *p) { return p; }
#elif CASE == 2
int *probe(void) { return 7; }
#elif CASE == 3
int probe(int *p) { return p; }
#elif CASE == 4
struct A { int n; };
struct B { int n; };
struct A probe(struct B b) { return b; }
#elif CASE == 5
int (*probe(int (*p)(int)))(long) { return p; }
#elif CASE == 6
int **probe(const int **p) { return p; }
#elif CASE == 7
int (*probe(int (*p)[3]))[2] { return p; }
#elif CASE == 8
int probe(void) { return; }
#elif CASE == 9
void probe(void) { return 1; }
#elif CASE == 10
int probe(void) { return (void)0; }
#elif CASE == 11
/* An incompatible expression in a dead arm still needs a valid source type. */
int *probe(long *p) { if (0) return p; return 0; }
#else
typedef int V __attribute__((vector_size(16)));
struct R { int x; };
static int calls;
static int object = 9;
static int target(void) { ++calls; return 7; }
static void act(void) { ++calls; return (void)0; }
static void forward(void) { return act(); }
static _Bool truth(int *p) { return p; }
static int *null_pointer(void) { return 0; }
static int *wide_null_pointer(void) { return (unsigned __int128)0; }
static int *signed_pointer(unsigned int *p) { return p; }
static int *qualified_pointer(const int *p) { return p; }
static int (*function_pointer(void *p))(void) { return p; }
static void *object_pointer(int *p) { return p; }
static const int *decayed_array(void) { static int a[2] = {5, 6}; return a; }
static struct R record_value(struct R r) { return r; }
static V vector_value(V v) { return v; }
static double floating(int x) { return x; }
static int integer(double x) { return x; }
static int *explicit_integer(void) { return (int *)7; }
static int *explicit_pointer(long *p) { return (int *)p; }
static int *wide_add(int *p, __int128 n) { return p + n; }
static int *wide_commuted(int *p, unsigned __int128 n) { return n + p; }
static int *wide_subtract(int *p, __int128 n) { return p - n; }
static int *wide_update(int *p, unsigned __int128 n) { p += n; p -= n; return p; }
static int *wide_null(int b, int *p) { return b ? p : (unsigned __int128)0; }
static int *wide_null_first(int b, int *p) { return b ? (__int128)0 : p; }
int main(void)
{
    unsigned int u = 3;
    const int c = 4;
    long l = 1;
    int a[3] = {1, 2, 3};
    V v = {1, 2, 3, 4};
    struct R r = {8};
    forward();
    if (function_pointer((void *)target)() != 7 || calls != 2) return 1;
    if (!truth(&object) || truth(null_pointer()) || truth(wide_null_pointer())) return 2;
    if ((void *)signed_pointer(&u) != &u || qualified_pointer(&c) != &c) return 3;
    if (object_pointer(&object) != &object || decayed_array()[1] != 6) return 4;
    if (record_value(r).x != 8 || vector_value(v)[2] != 3) return 5;
    if (floating(3) != 3.0 || integer(4.75) != 4) return 6;
    if ((unsigned long)explicit_integer() != 7 || (void *)explicit_pointer(&l) != &l) return 7;
    if (wide_add(a, 2) != &a[2] || wide_commuted(a, 1) != &a[1] || wide_subtract(a + 2, 2) != a) return 8;
    if (wide_update(a, 2) != a || wide_null(1, a) != a || wide_null(0, a) != 0) return 9;
    if (wide_null_first(0, a) != a || wide_null_first(1, a) != 0) return 10;
    return 0;
}
#endif
