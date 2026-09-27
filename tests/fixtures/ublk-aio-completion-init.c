/* Diagnostic-only payload adapter; keep the common root switch, timeout,
 * nonce and shutdown protocol, but execute a separate probe by preopened fd.
 * The original disk image and upstream program files are not edited. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>

static int probe_fd;
static int diagnostic_exec(const char *path, const char *arg, ...);
static _Noreturn void poweroff(void);
#define execl diagnostic_exec
#define main selftest_main
#include "selftest-init.c"
#undef main
#undef execl

static int diagnostic_exec(const char *path, const char *arg, ...)
{
    (void)arg;
    if (!strcmp(path, "/tests/run_kselftest.sh")) {
        char *argv[] = {"ublk-aio-completion-order", NULL};
        return fexecve(probe_fd, argv, environ);
    }
    errno = ENOSYS;
    return -1;
}

int main(void)
{
    probe_fd = open("/probe", O_RDONLY);
    if (probe_fd < 0)
        fatal("open completion probe");
    return selftest_main();
}
