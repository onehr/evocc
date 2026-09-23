/* Frozen vmaccess's attach cycle: exec holds cred_guard_mutex while waiting
 * for a ptraced sibling's reap. A second tracer thread breaks that cycle.
 * This diagnostic never patches the kernel or changes the upstream assertion. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ptrace.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static int report_pipe[2], ready_pipe[2];
static pid_t tracee, sibling, tracer;
static int reaped, reap_status, blocked_attach, blocked_exec;

static int stack_contains(pid_t tid, const char *needle)
{
    char path[96], stack[4096];
    snprintf(path, sizeof(path), "/proc/%d/stack", tid);
    int fd = open(path, O_RDONLY);
    if (fd < 0) return 0;
    ssize_t length = read(fd, stack, sizeof(stack) - 1);
    close(fd);
    if (length < 0) return 0;
    stack[length] = 0;
    printf("# stack tid=%d\n", tid);
    int found = 0;
    for (char *line = strtok(stack, "\n"); line; line = strtok(NULL, "\n")) {
        printf("# %s\n", line);
        if (strstr(line, needle)) found = 1;
    }
    return found;
}

static void *traced_thread(void *unused)
{
    (void)unused;
    pid_t tid = syscall(SYS_gettid);
    if (ptrace(PTRACE_TRACEME, 0, NULL, NULL)
        || write(report_pipe[1], &tid, sizeof(tid)) != sizeof(tid)) _exit(3);
    return NULL;
}

static void *release_exec(void *unused)
{
    (void)unused;
    usleep(300000);
    blocked_attach = stack_contains(tracer, "ptrace_attach");
    blocked_exec = stack_contains(tracee, "begin_new_exec");
    reaped = waitpid(sibling, &reap_status, __WALL);
    return NULL;
}

int main(int argc, char **argv)
{
    if (argc > 1) {
        alarm(10);
        for (;;) pause();
    }
    setbuf(stdout, NULL);
    alarm(10);
    tracer = syscall(SYS_gettid);
    if (pipe(report_pipe) || pipe(ready_pipe)) return 2;
    tracee = fork();
    if (tracee < 0) return 2;
    if (!tracee) {
        pthread_t thread;
        if (pthread_create(&thread, NULL, traced_thread, NULL)
            || pthread_join(thread, NULL)
            || write(ready_pipe[1], "x", 1) != 1) _exit(4);
        execl(argv[0], argv[0], "exec", (char *)NULL);
        _exit(5);
    }
    char ready;
    if (read(report_pipe[0], &sibling, sizeof(sibling)) != sizeof(sibling)
        || read(ready_pipe[0], &ready, 1) != 1) return 2;
    usleep(100000);
    printf("TAP version 13\n1..1\n");
    pthread_t reaper;
    if (pthread_create(&reaper, NULL, release_exec, NULL)) return 2;
    errno = 0;
    long attached = ptrace(PTRACE_ATTACH, tracee, NULL, NULL);
    int attach_errno = errno;
    if (pthread_join(reaper, NULL)) return 2;
    int status = 0;
    int stopped = attached == 0 && waitpid(tracee, &status, __WALL) == tracee
        && WIFSTOPPED(status);
    if (stopped && ptrace(PTRACE_DETACH, tracee, NULL, NULL)) return 2;
    kill(tracee, SIGKILL);
    int cleaned = waitpid(tracee, &status, __WALL) == tracee;
    errno = 0;
    int no_children = waitpid(-1, NULL, __WALL | WNOHANG) == -1 && errno == ECHILD;
    for (unsigned i = 0; i < 2; ++i) { close(report_pipe[i]); close(ready_pipe[i]); }
    int pass = blocked_attach && blocked_exec && reaped == sibling
        && WIFEXITED(reap_status) && WEXITSTATUS(reap_status) == 0
        && attached == 0 && stopped && cleaned && no_children;
    printf("# attach=%ld errno=%d blocked_attach=%d blocked_exec=%d sibling_reaped=%d stopped=%d cleaned=%d no_children=%d\n",
           attached, attach_errno, blocked_attach, blocked_exec, reaped == sibling,
           stopped, cleaned, no_children);
    printf("%s 1 sibling reap releases the exec/attach cycle\n", pass ? "ok" : "not ok");
    return pass ? 0 : 1;
}
