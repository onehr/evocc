/* AArch64 Linux API reduction: registration precedes the address check.
 * The companion selftest disables glibc registration before exec. Do not
 * unregister glibc's own area or silently apply that environment to the
 * unchanged direct upstream test. This diagnostic is reference-built. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/rseq.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static struct rseq area __attribute__((aligned(64)));

static int request(void *address, unsigned flags, int expected_errno)
{
    errno = 0;
    long rc = syscall(SYS_rseq, address, 32u, flags, RSEQ_SIG);
    int observed = errno;
    printf("# flags=%u rc=%ld errno=%d expected=%d\n",
           flags, rc, observed, expected_errno);
    return expected_errno ? rc == -1 && observed == expected_errno : rc == 0;
}

int main(int argc, char **argv)
{
    void *invalid = (void *)-4096UL; /* aligned, same address as upstream */
    if (argc == 2 && !strcmp(argv[1], "unregistered")) {
        if (__rseq_size != 0 || !request(invalid, 0, EFAULT))
            return 1;
        if (!request(&area, 0, 0))
            return 2;
        int ok = request(invalid, 0, EINVAL);
        ok &= request(&area, 0, EBUSY);
        ok &= request(&area, RSEQ_FLAG_UNREGISTER, 0);
        ok &= request(invalid, 0, EFAULT);
        return !ok;
    }
    printf("TAP version 13\n1..2\n");
    int registered = __rseq_size != 0 && request(invalid, 0, EINVAL);
    printf("%s 1 glibc registration precedes invalid-address check\n",
           registered ? "ok" : "not ok");
    fflush(stdout);
    pid_t child = fork();
    if (child == 0) {
        if (setenv("GLIBC_TUNABLES", "glibc.pthread.rseq=0", 1))
            _exit(100);
        execl("/proc/self/exe", argv[0], "unregistered", (char *)0);
        _exit(101);
    }
    int status = 0;
    int cleared = child > 0 && waitpid(child, &status, 0) == child
        && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    printf("%s 2 explicit register/unregister changes errno ordering\n",
           cleared ? "ok" : "not ok");
    return !(registered && cleared);
}
