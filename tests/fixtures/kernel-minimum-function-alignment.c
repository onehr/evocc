/* Machine alignment must not rewrite the source declaration's alignof. */
__attribute__((noinline, aligned(4))) int ordinary(int x) { return x + 1; }
__attribute__((noinline, cold, aligned(4))) int cold_entry(int x) { return x + 2; }
__attribute__((noinline, aligned(256))) int overaligned(int x) { return x + 3; }

int main(void)
{
    int (*volatile a)(int) = ordinary;
    int (*volatile b)(int) = cold_entry;
    int (*volatile c)(int) = overaligned;
    return __alignof__(ordinary) != 4 || __alignof__(cold_entry) != 4
        || __alignof__(overaligned) != 256
        || ((unsigned long)a & 127) || ((unsigned long)b & 127)
        || ((unsigned long)c & 255)
        || a(4) != 5 || b(4) != 6 || c(4) != 7;
}
