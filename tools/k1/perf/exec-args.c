/* Exec one installed selftest with fixed arguments.
 *
 * tools/k1/selftest-init.c runs each listed program without arguments. This
 * wrapper is a separate list entry for a bounded diagnostic workload; the
 * original program and its own entry stay unchanged. Build with
 * -DPROGRAM='"/tests/NAME"' -DARGUMENTS='"-m", "3"'.
 */
#include <stdio.h>
#include <unistd.h>

int main(void)
{
    char *argv[] = { PROGRAM, ARGUMENTS, NULL };
    execv(argv[0], argv);
    perror("execv");
    return 127;
}
