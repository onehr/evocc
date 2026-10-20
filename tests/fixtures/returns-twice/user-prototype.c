/* A user prototype marks a call that returns twice (D16). The name is not one
   of GCC's names, so only the attribute marks the call. checkpoint is a
   branch to _setjmp in my-checkpoint-*.s, so it saves the caller's context
   and returns twice. The pressure is the pressure of red-probe.c. main
   returns 0 when every kept value survives the jump. */
#include <setjmp.h>

int checkpoint(jmp_buf env) __attribute__((returns_twice));

static jmp_buf env;
volatile int sink;

__attribute__((noinline)) static void jump(void) { _longjmp(env, 1); }
__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 3 + 1; }

static int probe(int seed) {
    int a0 = opaque(seed), a1 = opaque(seed + 1), a2 = opaque(seed + 2),
        a3 = opaque(seed + 3), a4 = opaque(seed + 4), a5 = opaque(seed + 5),
        a6 = opaque(seed + 6), a7 = opaque(seed + 7), a8 = opaque(seed + 8),
        a9 = opaque(seed + 9), a10 = opaque(seed + 10), a11 = opaque(seed + 11),
        a12 = opaque(seed + 12), a13 = opaque(seed + 13);
    if (checkpoint(env) != 0)
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

int main(void) {
    int expected = 0;
    for (int i = 0; i < 14; i++)
        expected += 3 * (5 + i) + 1;
    return probe(5) == expected ? 0 : 1;
}
