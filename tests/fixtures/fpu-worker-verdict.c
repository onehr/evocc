/* Match the frozen FPU worker's plain-text/exit protocol, without a kernel
 * module. In particular, its missing-debugfs path prints SKIP but exits zero. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv)
{
    if (argc != 2)
        return 2;
    if (!strcmp(argv[1], "skip")) {
        puts("[SKIP]\tcan't access /sys/kernel/debug/selftest_helpers/test_fpu: No such file or directory");
        return 0;
    }
    if (!strcmp(argv[1], "fail")) {
        puts("[FAIL]\tusermode rounding mode clobbered");
        return 3;
    }
    if (!strcmp(argv[1], "ok")) {
        puts("[OK]\ttest_fpu");
        return 0;
    }
    return 2;
}
