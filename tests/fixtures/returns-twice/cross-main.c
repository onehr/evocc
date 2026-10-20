/* Calls that return twice across compilers (D16). Compile this unit and
   cross-other.c with any pair of compilers. In the first check, this unit
   calls setjmp and the other unit jumps. In the second, the other unit calls
   setjmp and this unit jumps. main returns 0, or the number of the first
   check that failed. */
#include <setjmp.h>

void other_jump(jmp_buf *env, int value);
int other_catch(int seed, void (*jumper)(jmp_buf *, int));

static jmp_buf env;
volatile int sink;

__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 5 + 1; }

static void main_jump(jmp_buf *target, int value) { longjmp(*target, value); }

static int caught_here(int seed) {
    int a0 = opaque(seed), a1 = opaque(seed + 1), a2 = opaque(seed + 2),
        a3 = opaque(seed + 3), a4 = opaque(seed + 4), a5 = opaque(seed + 5),
        a6 = opaque(seed + 6), a7 = opaque(seed + 7), a8 = opaque(seed + 8),
        a9 = opaque(seed + 9), a10 = opaque(seed + 10), a11 = opaque(seed + 11);
    int r = setjmp(env);
    if (r != 0)
        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + r;
    int t0 = opaque(1), t1 = opaque(2), t2 = opaque(3), t3 = opaque(4),
        t4 = opaque(5), t5 = opaque(6), t6 = opaque(7), t7 = opaque(8),
        t8 = opaque(9), t9 = opaque(10), t10 = opaque(11), t11 = opaque(12);
    sink = t0 + t1 + t2 + t3 + t4 + t5 + t6 + t7 + t8 + t9 + t10 + t11;
    other_jump(&env, 3);
    return -1;
}

int main(void) {
    int expected = 3;
    for (int i = 0; i < 12; i++)
        expected += (4 + i) * 5 + 1;
    if (caught_here(4) != expected)
        return 1;
    if (other_catch(6, main_jump) != 6 * 3 + 2 + 4)
        return 2;
    return 0;
}
