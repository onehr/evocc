/* Prototype arguments must obey the same conversion profile as initialization. */
#ifndef CASE
#define CASE 0
#endif

#if CASE == 1
void take(int *);
void probe(long *p) { take(p); }
#elif CASE == 2
void take(int *);
void probe(void) { take(7); }
#elif CASE == 3
void take(int);
void probe(int *p) { take(p); }
#elif CASE == 4
struct A { int n; };
struct B { int n; };
void take(struct A);
void probe(struct B b) { take(b); }
#elif CASE == 5
void take(int (*)(long));
void probe(int (*p)(int)) { take(p); }
#elif CASE == 6
void take(int **);
void probe(const int **p) { take(p); }
#elif CASE == 7
void take(int (*)[2]);
void probe(int (*p)[3]) { take(p); }
#elif CASE == 8
void probe(void (*take)(int *), long *p) { take(p); }
#elif CASE == 9
void take(int *);
void probe(long *p) { if (0) take(p); }
#else
typedef union __attribute__((transparent_union)) { int *p; void *v; } U;
struct R { int x; };
typedef int V __attribute__((vector_size(16)));
static int calls;
static int object = 3;
static void take(int *p) { if (p == &object) ++calls; }
static int arithmetic(double d, int x) { return d == 2.0 && x == 4; }
static int aggregate(struct R r, V v) { return r.x + v[1]; }
static void transparent(U u) { take(u.p); }
static int target(void) { return 9; }
static int callback(int (*p)(void)) { return p(); }
static int forward_list(__builtin_va_list ap) { return __builtin_va_arg(ap, int); }
static int variadic(int n, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    int value = forward_list(ap);
    int original = __builtin_va_arg(ap, int);
    __builtin_va_end(ap);
    return value == 19 && original == 19;
}
void unprototyped();
/* At this call site there is no prototype; float is promoted to double. */
static void old_style_control(void) { unprototyped(1, 2.0f); }
void unprototyped(int x, double y) { if (x == 1 && y == 2.0) ++calls; }
int main(void)
{
    unsigned int u = 0;
    const int c = 0;
    long l = 0;
    struct R r = {5};
    V v = {1, 2, 3, 4};
    void (*fp)(int *) = take;
    take(&object); fp(&object); transparent(&object); old_style_control();
    take(0); take((__int128)0); take(&u); take(&c); take((int *)&l); take((int *)7);
    if (calls != 4 || !arithmetic(2, 4.75)) return 1;
    if (aggregate(r, v) != 7 || callback((void *)target) != 9) return 2;
    if (!variadic(1, 19)) return 3;
    return 0;
}
#endif
