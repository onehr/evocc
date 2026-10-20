/* Calls that return twice (EvoIR 3.10, D16). main returns 0, or the number
   of the first check that failed. C11 7.13.2.1p3 makes a non-volatile
   automatic object that changed between setjmp and longjmp indeterminate,
   so no check reads one after the jump. */
#include <setjmp.h>
#include <signal.h>

static jmp_buf env;
static jmp_buf plain;
static sigjmp_buf masked_env;
volatile int sink;

__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 7 + 3; }
__attribute__((noinline)) static void jump(int value) { longjmp(env, value); }
__attribute__((noinline)) static void plain_jump(int value) { _longjmp(plain, value); }
__attribute__((noinline)) static void masked_jump(int value) { siglongjmp(masked_env, value); }

/* Each recursion level is one more frame between setjmp and longjmp. */
__attribute__((noinline)) static void deep(int depth, int value) {
    if (depth == 0)
        longjmp(env, value);
    deep(depth - 1, value + 1);
    sink = depth;
}

/* Unchanged values read only after the second return. */
static int kept(int seed) {
    int a = opaque(seed), b = opaque(seed + 1), c = opaque(seed + 2);
    int r = setjmp(env);
    if (r != 0)
        return a + b + c + r;
    int t0 = opaque(1), t1 = opaque(2), t2 = opaque(3), t3 = opaque(4);
    sink = t0 + t1 + t2 + t3;
    jump(5);
    return -1;
}

/* A volatile object keeps the value it had when longjmp was called. */
static int changed(int seed) {
    volatile int count = 0;
    int base = opaque(seed);
    if (setjmp(env) != 0)
        return count * 1000 + base;
    count = 41;
    count++;
    jump(1);
    return -1;
}

/* setjmp inside a loop; each iteration jumps back to its own call once. */
static int looped(int seed) {
    volatile int iterations = 0;
    volatile int jumps = 0;
    int fixed = opaque(seed);
    for (volatile int i = 0; i < 4; i++) {
        int before = opaque(i);
        if (setjmp(env) == 0) {
            iterations++;
            jump(i + 1);
        }
        jumps++;
        if (before != i * 7 + 3)
            return -2;
    }
    return iterations * 100 + jumps * 10 + (fixed == seed * 7 + 3);
}

/* longjmp from three nested frames below the caller of setjmp. */
static int nested(int seed) {
    int a = opaque(seed);
    int r = setjmp(env);
    if (r != 0)
        return r * 100 + (a == seed * 7 + 3);
    deep(3, 1);
    return -1;
}

/* _setjmp and _longjmp do not save the signal mask. */
static int underscore(int seed) {
    int a = opaque(seed);
    int r = _setjmp(plain);
    if (r != 0)
        return r + a;
    sink = opaque(1) + opaque(2);
    plain_jump(7);
    return -1;
}

static int masked(int seed, int savemask) {
    int a = opaque(seed);
    int r = sigsetjmp(masked_env, savemask);
    if (r != 0)
        return r + a;
    sink = opaque(3);
    masked_jump(9);
    return -1;
}

/* siglongjmp restores the mask that sigsetjmp saved. */
static int restored(void) {
    sigset_t blocked, now;
    sigemptyset(&blocked);
    sigaddset(&blocked, SIGUSR1);
    sigprocmask(SIG_UNBLOCK, &blocked, 0);
    if (sigsetjmp(masked_env, 1) != 0) {
        sigprocmask(SIG_BLOCK, 0, &now);
        return sigismember(&now, SIGUSR1);
    }
    sigprocmask(SIG_BLOCK, &blocked, 0);
    masked_jump(1);
    return -1;
}

int main(void) {
    if (kept(2) != 17 + 24 + 31 + 5)
        return 1;
    if (changed(10) != 42 * 1000 + 73)
        return 2;
    if (looped(5) != 441)
        return 3;
    if (nested(6) != 401)
        return 4;
    if (underscore(8) != 7 + 59)
        return 5;
    if (masked(9, 0) != 9 + 66)
        return 6;
    if (masked(9, 1) != 9 + 66)
        return 7;
    if (restored() != 0)
        return 8;
    return 0;
}
