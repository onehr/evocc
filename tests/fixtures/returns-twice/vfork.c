/* vfork returns twice (D16). The child runs in the parent's memory until
   _exit, and its code is the code of the parent's function. The parent keeps
   fourteen values across vfork, more than the callee-saved registers hold.
   The child keeps forty values live at once without a call, so it needs
   stack slots too. They must not be the parent's slots. main returns 0 when
   the parent's values and the child's exit status are right. */
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

volatile int sink;
volatile int level = 1;

__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 11 + 4; }

static int forked(int seed) {
    int a0 = opaque(seed), a1 = opaque(seed + 1), a2 = opaque(seed + 2),
        a3 = opaque(seed + 3), a4 = opaque(seed + 4), a5 = opaque(seed + 5),
        a6 = opaque(seed + 6), a7 = opaque(seed + 7), a8 = opaque(seed + 8),
        a9 = opaque(seed + 9), a10 = opaque(seed + 10), a11 = opaque(seed + 11),
        a12 = opaque(seed + 12), a13 = opaque(seed + 13);
    pid_t pid = vfork();
    if (pid < 0)
        return -1;
    if (pid == 0) {
        int c0 = level, c1 = level + 1, c2 = level + 2, c3 = level + 3,
            c4 = level + 4, c5 = level + 5, c6 = level + 6, c7 = level + 7,
            c8 = level + 8, c9 = level + 9, c10 = level + 10, c11 = level + 11,
            c12 = level + 12, c13 = level + 13, c14 = level + 14,
            c15 = level + 15, c16 = level + 16, c17 = level + 17,
            c18 = level + 18, c19 = level + 19, c20 = level + 20,
            c21 = level + 21, c22 = level + 22, c23 = level + 23,
            c24 = level + 24, c25 = level + 25, c26 = level + 26,
            c27 = level + 27, c28 = level + 28, c29 = level + 29,
            c30 = level + 30, c31 = level + 31, c32 = level + 32,
            c33 = level + 33, c34 = level + 34, c35 = level + 35,
            c36 = level + 36, c37 = level + 37, c38 = level + 38,
            c39 = level + 39;
        _exit(((c39 ^ (c38 + (c37 ^ (c36 + (c35 ^ (c34 + (c33 ^ (c32 + (c31 ^
            (c30 + (c29 ^ (c28 + (c27 ^ (c26 + (c25 ^ (c24 + (c23 ^ (c22 +
            (c21 ^ (c20 + (c19 ^ (c18 + (c17 ^ (c16 + (c15 ^ (c14 + (c13 ^
            (c12 + (c11 ^ (c10 + (c9 ^ (c8 + (c7 ^ (c6 + (c5 ^ (c4 + (c3 ^ (c2
            + (c1 ^ c0)))))))))))))))))))))))))))))))))))))))) & 0x7f);
    }
    int status = 0;
    if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status))
        return -2;
    return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12
           + a13 + WEXITSTATUS(status) * 100000;
}

int main(void) {
    int expected = 0;
    for (int i = 0; i < 14; i++)
        expected += (2 + i) * 11 + 4;
    int child = 1;
    for (int i = 1; i < 40; i++)
        child = i % 2 ? (1 + i) ^ child : (1 + i) + child;
    expected += (child & 0x7f) * 100000;
    return forked(2) == expected ? 0 : 1;
}
