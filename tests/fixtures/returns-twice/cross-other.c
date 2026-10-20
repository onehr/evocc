/* The other unit of cross-main.c. */
#include <setjmp.h>

static jmp_buf other_env;
volatile int other_sink;

__attribute__((noinline)) static int other_opaque(int v) {
    other_sink = v;
    return v * 3 + 2;
}

void other_jump(jmp_buf *env, int value) { longjmp(*env, value); }

int other_catch(int seed, void (*jumper)(jmp_buf *, int)) {
    int a = other_opaque(seed);
    int r = setjmp(other_env);
    if (r != 0)
        return a + r;
    other_sink = other_opaque(1) + other_opaque(2);
    jumper(&other_env, 4);
    return -1;
}
