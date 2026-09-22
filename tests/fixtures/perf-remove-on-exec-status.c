/* Diagnostic reduction of upstream enable_then_fork_exec, not its replacement.
 * Keep the event attributes and 100 ms observation; expose the child's status
 * instead of inferring a cause from a nonzero waitpid result. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/perf_event.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile sig_atomic_t signal_count;
static volatile sig_atomic_t signal_code;

static void handler(int sig, siginfo_t *info, void *context)
{
    (void)sig;
    (void)context;
    signal_code = info->si_code;
    signal_count++;
}

static int install_handler(void)
{
    struct sigaction action = {0};
    action.sa_flags = SA_SIGINFO | SA_NODEFER;
    action.sa_sigaction = handler;
    sigemptyset(&action.sa_mask);
    return sigaction(SIGTRAP, &action, NULL);
}

int main(int argc, char **argv)
{
    (void)argc;
    if (!strcmp(argv[0], "exec_child")) {
        if (install_handler())
            return 121;
        while (!signal_count) {}
        /* 122 distinguishes a post-exec delivered trap from exec failure. */
        return 122;
    }
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..32");
    if (install_handler()) {
        perror("sigaction");
        return 1;
    }
    int failures = 0;
    for (int round = 1; round <= 32; round++) {
        struct perf_event_attr attr = {
            .type = PERF_TYPE_HARDWARE,
            .size = sizeof(attr),
            .config = PERF_COUNT_HW_INSTRUCTIONS,
            .sample_period = 1000,
            .exclude_kernel = 1,
            .exclude_hv = 1,
            .disabled = 1,
            .inherit = 1,
            .remove_on_exec = 1,
            .sigtrap = 1,
        };
        int fd = syscall(__NR_perf_event_open, &attr, 0, -1, -1,
                         PERF_FLAG_FD_CLOEXEC);
        if (fd < 0 || ioctl(fd, PERF_EVENT_IOC_ENABLE, 0)) {
            perror("open/enable event");
            return 1;
        }
        pid_t child = fork();
        if (child < 0) {
            perror("fork");
            close(fd);
            return 1;
        }
        if (!child) {
            execl("/proc/self/exe", "exec_child", (char *)NULL);
            _exit(120);
        }
        usleep(100000);
        int status = 0;
        errno = 0;
        pid_t observed = waitpid(child, &status, WNOHANG);
        int error = errno;
        close(fd);
        int stopped = 0;
        if (observed == 0) {
            stopped = kill(child, SIGKILL);
            while (waitpid(child, &status, 0) < 0 && errno == EINTR) {}
        }
        int passed = observed == 0 && stopped == 0;
        printf("# round=%d child=%d wait=%d errno=%d status=0x%x "
               "exit=%d signal=%d parent_traps=%d code=%d\n",
               round, child, observed, error, status,
               WIFEXITED(status) ? WEXITSTATUS(status) : -1,
               WIFSIGNALED(status) ? WTERMSIG(status) : 0,
               signal_count, signal_code);
        printf("%s %d remove_on_exec child remains alive\n",
               passed ? "ok" : "not ok", round);
        failures += !passed;
        if (observed < 0) {
            kill(child, SIGKILL);
            while (waitpid(child, NULL, 0) < 0 && errno == EINTR) {}
        }
    }
    return failures != 0;
}
