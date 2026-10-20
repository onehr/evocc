/* D16 RED probe: values computed before setjmp and read only after the
   second return. Fourteen of them do not fit the ten callee-saved registers,
   so some live in stack slots. The first-return path keeps fourteen
   temporaries live across calls, so it needs slots too. The compiler of
   4d856ea9 lets the two kinds share slots, because the first kind is dead on
   the first-return path, and the second return then reads temporaries.
   main returns 0 when every kept value survives the jump. */
#include <setjmp.h>

static jmp_buf env;
volatile int sink;

__attribute__((noinline)) static void jump(void) { longjmp(env, 1); }
__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 3 + 1; }

int probe(int seed) {
    int a0 = opaque(seed), a1 = opaque(seed + 1), a2 = opaque(seed + 2),
        a3 = opaque(seed + 3), a4 = opaque(seed + 4), a5 = opaque(seed + 5),
        a6 = opaque(seed + 6), a7 = opaque(seed + 7), a8 = opaque(seed + 8),
        a9 = opaque(seed + 9), a10 = opaque(seed + 10), a11 = opaque(seed + 11),
        a12 = opaque(seed + 12), a13 = opaque(seed + 13);
    if (setjmp(env) != 0)
        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11
               + a12 + a13;
    int t0 = opaque(100), t1 = opaque(101), t2 = opaque(102), t3 = opaque(103),
        t4 = opaque(104), t5 = opaque(105), t6 = opaque(106), t7 = opaque(107),
        t8 = opaque(108), t9 = opaque(109), t10 = opaque(110), t11 = opaque(111),
        t12 = opaque(112), t13 = opaque(113);
    sink = t0 + t1 + t2 + t3 + t4 + t5 + t6 + t7 + t8 + t9 + t10 + t11 + t12
           + t13;
    jump();
    return -1;
}

int main(int argc, char **argv) {
    (void)argv;
    int seed = argc - 1;
    int expected = 0;
    for (int i = 0; i < 14; i++)
        expected += 3 * (seed + i) + 1;
    return probe(seed) == expected ? 0 : 1;
}
