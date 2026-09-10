#if CASE == 1
union value { long member; };
union value f(void) { return (union value)1; }
#elif CASE == 2
struct a { int member; };
struct b { int member; };
union value { struct a member; };
union value f(struct b b) { return (union value)b; }
#elif CASE == 3
union value { char *member; };
union value f(const char *p) { return (union value)p; }
#elif CASE == 4
union value { int member; };
int f(void) { ((union value)1).member = 2; return 0; }
#elif CASE == 5
union value { void *member; };
union value f(void) { return (union value)0; }
#elif CASE == 6
union value { int member; };
union value *f(int n) { return &(union value)n; }
#elif CASE == 7
union value { int member : 3; };
union value f(void) { return (union value)7; }
#elif CASE == 8
union value { int member : 32; };
union value f(void) { return (union value)7; }
#endif
