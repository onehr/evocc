/* The FORTIFY query follows a passing KUnit-style assertion macro. */
typedef unsigned long size_t;
#define IS_ICE(x) (sizeof(int) == sizeof(*(8 ? ((void *)((long)(x) * 0l)) : (int *)8)))
#define COMPILE_LENGTH(p) ({ \
    char *q = (char *)(p); \
    size_t result = ~(size_t)0; \
    const size_t size = __builtin_dynamic_object_size(p, 1); \
    if (size != ~(size_t)0 && __builtin_constant_p(*q)) { \
        size_t last = size - 1; \
        if (__builtin_constant_p(q[last]) && q[last] == '\0') \
            result = __builtin_strlen(q); \
    } \
    result; \
})
static int failures;
struct context { const char *file; int line; };
__attribute__((noinline)) static void fail(void) { ++failures; }
__attribute__((noinline)) static int probe(struct context *test)
{
    char stack[80] = "Test!";
    do {
        *(const char *volatile *)&test->file = "here";
        *(volatile int *)&test->line = 90;
        if (__builtin_expect(!!IS_ICE(__builtin_strlen(stack)) == 0, 1))
            break;
        fail();
    } while (0);
    return COMPILE_LENGTH(stack) != 5;
}
int main(void)
{
    struct context context = {0};
    return probe(&context) + failures;
}
