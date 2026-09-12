extern void constant_required(void) __attribute__((error("constant required")));
static int calls;
static int observe(void) { ++calls; return 4; }
static void mutate(int *p) { *p = 9; }

unsigned minimum(unsigned x)
{
    return ({
        __auto_type a = x;
        __auto_type b = 2;
        if (!(__builtin_constant_p((long long)b >= 0) && (long long)b >= 0))
            constant_required();
        a < b ? a : b;
    });
}

static __attribute__((always_inline)) inline unsigned request(unsigned req)
{
    if (!__builtin_constant_p(req))
        constant_required();
    return req;
}

int main(void)
{
    unsigned int x = 5;
    int side = 0;
    int changing = 1;
    volatile int v = 3;
    observe();
    unsigned int result = ({
        __auto_type a = x;
        __auto_type b = 2;
        if (!(__builtin_constant_p((long long)b >= 0) && (long long)b >= 0))
            constant_required();
        a < b ? a : b;
    });
    unsigned int req = request(({ ++side; (unsigned)(7 | 1024); }));
    mutate(&changing);
    return result != 2 || minimum(5) != 2 || minimum(1) != 1
        || req != 1031 || side != 1 || calls != 1 || changing != 9
        || __builtin_constant_p(side++) || __builtin_constant_p(observe())
        || __builtin_constant_p(({ ++side; 7; })) || __builtin_constant_p(v)
        || side != 1 || calls != 1;
}
