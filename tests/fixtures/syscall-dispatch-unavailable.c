/* Diagnostic for the frozen arm64 kernel's !CONFIG_GENERIC_ENTRY stubs.
 * EINVAL is the expected observation here, not a successful dispatch test.
 * Only our own stopped child is traced; every child is resumed and reaped. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/ptrace.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static int number, failures;

static void check(int condition, const char *name)
{
    printf("%s %d - %s\n", condition ? "ok" : "not ok", ++number, name);
    failures += !condition;
}

static long trace(int operation, pid_t child, void *size, void *data)
{
    return syscall(SYS_ptrace, operation, child, size, data);
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..7");
    unsigned char selector = SYSCALL_DISPATCH_FILTER_ALLOW;
    errno = 0;
    int result = prctl(PR_SET_SYSCALL_USER_DISPATCH, PR_SYS_DISPATCH_ON,
                       0UL, 0UL, &selector);
    printf("# dispatch enable returned=%d errno=%d\n", result, errno);
    check(result == -1 && errno == EINVAL, "arm64 dispatch enable is unavailable");
    /* If run on a supported kernel, never leave dispatch enabled. */
    errno = 0;
    result = prctl(PR_SET_SYSCALL_USER_DISPATCH, PR_SYS_DISPATCH_OFF,
                   0UL, 0UL, (void *)0);
    printf("# dispatch disable returned=%d errno=%d\n", result, errno);
    check(result == -1 && errno == EINVAL, "even disable reaches the unavailable stub");
    errno = 0;
    result = prctl(PR_SET_SYSCALL_USER_DISPATCH, PR_SYS_DISPATCH_ON,
                   0UL, 0UL, (void *)1);
    check(result == -1 && errno == EINVAL, "unsupported API precedes selector access");

    pid_t child = fork();
    if (child == 0) {
        if (trace(PTRACE_TRACEME, 0, NULL, NULL)) _exit(2);
        raise(SIGSTOP);
        _exit(0);
    }
    int status = 0;
    pid_t waited = child > 0 ? waitpid(child, &status, 0) : -1;
    int stopped = waited == child && child > 0 && WIFSTOPPED(status)
                  && WSTOPSIG(status) == SIGSTOP;
    check(stopped, "ordinary tracing reaches our child's stop");
    struct ptrace_sud_config config;
    memset(&config, 0xa5, sizeof(config));
    struct ptrace_sud_config before = config;
    errno = 0;
    long got = stopped ? trace(PTRACE_GET_SYSCALL_USER_DISPATCH_CONFIG, child,
                               (void *)sizeof(config), &config) : 0;
    printf("# ptrace get returned=%ld errno=%d\n", got, errno);
    check(got == -1 && errno == EINVAL && !memcmp(&before, &config, sizeof(config)),
          "unavailable ptrace get preserves destination bytes");
    memset(&config, 0, sizeof(config));
    config.mode = PR_SYS_DISPATCH_ON;
    config.offset = 0x400000;
    config.len = 0x1000;
    errno = 0;
    long set = stopped ? trace(PTRACE_SET_SYSCALL_USER_DISPATCH_CONFIG, child,
                               (void *)sizeof(config), &config) : 0;
    printf("# ptrace set returned=%ld errno=%d\n", set, errno);
    check(set == -1 && errno == EINVAL, "unavailable ptrace set rejects a valid descriptor");
    int resumed = stopped && !trace(PTRACE_CONT, child, NULL, NULL);
    if (resumed) waited = waitpid(child, &status, 0);
    else if (child > 0 && stopped) {
        kill(child, SIGKILL);
        waited = waitpid(child, &status, 0);
    }
    check(resumed && waited == child && WIFEXITED(status) && !WEXITSTATUS(status),
          "ordinary tracing resumes and reaps the child");
    return failures != 0;
}
