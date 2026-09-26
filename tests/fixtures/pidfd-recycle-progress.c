/* Bounded K11 reduction of pidfd_test's fork/recycle path. No PID sysctl,
 * original test budget, or kernel source is changed. This measures progress,
 * not completion of the original 32768-cycle recycling test. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <sys/mount.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static double now(void)
{
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t))
        return -1;
    return t.tv_sec + t.tv_nsec * 1e-9;
}

static int reap(pid_t child)
{
    int status;
    pid_t result;
    do {
        result = waitpid(child, &status, 0);
    } while (result < 0 && errno == EINTR);
    return result == child && WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

static int measure_progress(void)
{
    char path[80];
    FILE *file = fopen("/proc/sys/kernel/pid_max", "r");
    int pid_max = -1;
    if (file) {
        if (fscanf(file, "%d", &pid_max) != 1)
            pid_max = -1;
        fclose(file);
    }
    printf("TAP version 13\n1..3\n# pid_max=%d\n", pid_max);
    fflush(stdout);
    int syncfd[2];
    if (pipe2(syncfd, O_CLOEXEC))
        return 1;
    pid_t retired = fork();
    if (retired == 0) {
        char c;
        close(syncfd[1]);
        (void)read(syncfd[0], &c, 1);
        _exit(0);
    }
    close(syncfd[0]);
    if (retired < 0) {
        close(syncfd[1]);
        return 1;
    }
    snprintf(path, sizeof(path), "/proc/%d", retired);
    int pidfd = open(path, O_DIRECTORY | O_CLOEXEC);
    close(syncfd[1]);
    int reaped_retired = reap(retired);
    int retired_ok = pidfd >= 0 && reaped_retired;
    printf("%s 1 retired process retains a proc pid descriptor\n",
           retired_ok ? "ok" : "not ok");
    fflush(stdout);
    if (!retired_ok) {
        if (pidfd >= 0)
            close(pidfd);
        return 1;
    }

    unsigned completed = 0, stale = 0;
    pid_t first = -1, last = -1;
    double start = now();
    for (unsigned i = 0; i < 1024; ++i) {
        int p[2];
        if (pipe2(p, O_CLOEXEC))
            break;
        pid_t child = fork();
        if (child == 0) {
            char c;
            close(p[1]);
            (void)read(p[0], &c, 1);
            _exit(0);
        }
        close(p[0]);
        if (child < 0) {
            close(p[1]);
            break;
        }
        if (first < 0)
            first = child;
        last = child;
        int stopped = kill(child, SIGSTOP) == 0;
        close(p[1]);
        errno = 0;
        int rejected = syscall(SYS_pidfd_send_signal, pidfd, SIGCONT, NULL, 0) == -1
            && errno == ESRCH;
        int continued = kill(child, SIGCONT) == 0;
        if (!continued)
            (void)kill(child, SIGKILL);
        int reaped = reap(child);
        if (!stopped || !continued || !reaped)
            break;
        ++completed;
        stale += rejected;
    }
    double elapsed = now() - start;
    close(pidfd);
    printf("# rounds=%u stale_ESRCH=%u first=%d last=%d seconds=%.6f\n",
           completed, stale, first, last, elapsed);
    printf("%s 2 all 1024 stop continue and reap cycles complete\n",
           completed == 1024 ? "ok" : "not ok");
    printf("%s 3 retired descriptor reports ESRCH during later fork cycles\n",
           stale == 1024 ? "ok" : "not ok");
    return completed != 1024 || stale != 1024;
}

int main(void)
{
#ifdef FRESH_NAMESPACE
    /* The upstream recycling case creates its own PID and mount namespaces.
     * Observe that namespace's real pid_max; do not infer it from the root. */
    if (unshare(CLONE_NEWPID | CLONE_NEWNS)
        || mount(NULL, "/", NULL, MS_REC | MS_PRIVATE, NULL))
        return 2;
    pid_t child = fork();
    if (child == 0) {
        if (umount2("/proc", MNT_DETACH)
            || mount("proc", "/proc", "proc", 0, NULL))
            _exit(3);
        int result = measure_progress();
        fflush(stdout);
        _exit(result);
    }
    return child < 0 || !reap(child);
#else
    return measure_progress();
#endif
}
