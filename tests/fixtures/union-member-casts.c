struct pair { long a, b; };
union value { int integer; unsigned long long wide; int *pointer; struct pair pair; };
union callable { int (*function)(int); unsigned long address; };
union text { const char *pointer; unsigned long address; };
static union value saved = (union value)31;
static int saved_object = 41;
static union value saved_pointer = (union value)&saved_object;
static int calls;
static int next(void) { ++calls; return 7; }
static int plus(int n) { return n + 3; }
static union value make(struct pair p) { return (union value)p; }
static long sum(union value value) { return value.pair.a + value.pair.b; }

int main(void) {
    const int ci = 11;
    volatile int vi = 13;
    int object = 19;
    struct pair pair = {23, 29};
    union value a = (union value)next();
    union value b = (union value)0x100000000ULL;
    union value p = (union value)&object;
    union value c = make(pair);
    union callable f = (union callable)plus;
    union text t = (union text)(const char *)"ok";
    if (a.integer != 7 || calls != 1) return 1;
    if (b.wide != 0x100000000ULL || *p.pointer != 19) return 2;
    if (c.pair.a != 23 || c.pair.b != 29 || sum((union value)pair) != 52) return 3;
    if (((union value)ci).integer != 11 || ((union value)vi).integer != 13) return 4;
    if (f.function(4) != 7 || t.pointer[1] != 'k') return 5;
    if (saved.integer != 31 || *saved_pointer.pointer != 41) return 6;
    if (sizeof((union value)next()) != sizeof(union value) || calls != 1) return 7;
    return 0;
}
