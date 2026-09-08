_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_abs(1)), int), "int result");
_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_labs(1)), long), "long result");
_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_llabs(1)), long long), "long long result");
static int calls;
static int next(void) { ++calls; return -7; }
int main(void) {
    volatile int i=-17;
    volatile long l=-4294967297L;
    volatile long long ll=-4294967299LL;
    if (__builtin_abs(i)!=17 || __builtin_labs(l)!=4294967297L || __builtin_llabs(ll)!=4294967299LL) return 1;
    if (__builtin_abs(next())!=7 || calls!=1) return 2;
    int x=-3;
    if (__builtin_abs(x++)!=3 || x!=-2) return 3;
    if (__builtin_abs(++x)!=1 || x!=-1) return 4;
    for (int n=0;n<3;++n) if (__builtin_abs(next())!=7) return 5;
    if (calls!=4) return 6;
    if (__builtin_abs(-2147483647)!=2147483647 || __builtin_labs(-9223372036854775807L)!=9223372036854775807L || __builtin_llabs(-9223372036854775807LL)!=9223372036854775807LL) return 7;
    if (__builtin_abs(0)!=0 || __builtin_labs(0L)!=0 || __builtin_llabs(0LL)!=0) return 8;
    if (__builtin_abs((long)4294967295)!=1) return 9;
    return 0;
}
