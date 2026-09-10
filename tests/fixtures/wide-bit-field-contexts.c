#include <stdio.h>

struct U { unsigned long x:40; };
struct S { signed long pad:12; signed long x:52; };

int main(void) {
    volatile struct U u = {0xffffffffffUL};
    printf("comparison %d %d %d\n", u.x > 1, u.x != 1, !!u.x);
    printf("right-shift %lx\n", (unsigned long)(u.x >> 8));
    printf("wide-product %lx\n", (unsigned long)(u.x * 2UL));
    printf("wide-sum %lx\n", (unsigned long)(u.x + 1UL));
    printf("narrow-difference-sign %d\n", (u.x - 2) < 0);
    printf("wide-condition %lx\n", (unsigned long)(1 ? u.x : 0));
    printf("compound %lx ", (unsigned long)(u.x += 1));
    printf("%lx\n", (unsigned long)u.x);
    u.x = 0xffffffffffUL;
    printf("postfix %lx ", (unsigned long)u.x++);
    printf("%lx\n", (unsigned long)u.x);
    u.x = 0xffffffffffUL;
    printf("prefix %lx\n", (unsigned long)++u.x);
    struct S s = {0x123, 0x123456789ABCDL};
    s.x ^= 0x0008765412345678L;
    printf("signed-store %lx %lx\n", (unsigned long)s.pad, (unsigned long)s.x);
    u.x = 0xffffffffffUL;
    printf("conflict-shift %lx\n", (unsigned long)(u.x << 1UL));
    printf("conflict-sum %lx\n", (unsigned long)(u.x + 1));
    printf("conflict-unary-nested %lx\n", (unsigned long)(+u.x + 1));
    printf("conflict-condition-nested %lx\n", (unsigned long)((1 ? u.x : 0) + 1));
    printf("conflict-compound-nested %lx\n", (unsigned long)((u.x += 0) + 1));
    printf("conflict-postfix-nested %lx\n", (unsigned long)(u.x++ + 1));
    u.x = 0xffffffffffUL;
    printf("conflict-comma-nested %lx\n", (unsigned long)((0, u.x) + 1));
    printf("conflict-negative-arm %lx\n", (unsigned long)(0 ? u.x : -1));
    printf("divide-assign %lx\n", (unsigned long)(u.x /= -1));
    u.x = 0xffffffffffUL;
    printf("remainder-assign %lx\n", (unsigned long)(u.x %= -1));
    u.x = 0xffffffffffUL;
    printf("negative-comparisons %d %d %d %d\n", u.x < -1, u.x == -1, u.x < -1L, u.x == -1L);
    printf("signed-wide-divide %lx %lx\n", (unsigned long)(u.x / -1L), (unsigned long)(u.x % -1L));
    printf("signed-wide-condition %lx\n", (unsigned long)(0 ? u.x : -1L));
    printf("signed-wide-condition-sign %d\n", (0 ? u.x : -1L) < 0);
    printf("signed-wide-sum-sign %d\n", (u.x + -0x10000000000L) < 0);
    return 0;
}
