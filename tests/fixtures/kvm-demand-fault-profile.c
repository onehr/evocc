/* TODO K02 diagnostic, not a replacement for the full 120-second test.
 * Execute the installed, unchanged kselftest with one 256 KiB guest mode and
 * trace only the stage-2 fault handler. Build with the reference compiler.
 * Use the existing selftest-init and a small read-only initramfs payload. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/wait.h>
#include <unistd.h>

#define TRACE "/sys/kernel/tracing/"
static unsigned traced_faults;

static void fail(const char *what)
{
    printf("not ok 1 diagnostic setup: %s: %s\n", what, strerror(errno));
    exit(1);
}

static void control(const char *name, const char *value)
{
    char path[256];
    if (snprintf(path, sizeof(path), TRACE "%s", name) >= (int)sizeof(path))
        fail("trace path");
    int fd = open(path, O_WRONLY | O_TRUNC);
    if (fd < 0)
        fail(path);
    size_t size = strlen(value);
    if (write(fd, value, size) != (ssize_t)size || close(fd))
        fail(path);
}

static void report(const char *path)
{
    FILE *file = fopen(path, "r");
    if (!file)
        fail(path);
    printf("# BEGIN %s\n", path);
    char line[2048];
    size_t bytes = 0;
    while (fgets(line, sizeof(line), file)) {
        bytes += strlen(line);
        /* A diagnostic must not silently accept a truncated trace or fill
         * the host disk with an unbounded execution log. */
        if (bytes > 2 * 1024 * 1024) {
            errno = EFBIG;
            fail("diagnostic exceeds 2 MiB per-file bound");
        }
        printf("# %s", line);
        if (strstr(line, "kvm_handle_guest_abort() {"))
            traced_faults++;
        unsigned long lost;
        if ((sscanf(line, "overrun: %lu", &lost) == 1
             || sscanf(line, "commit overrun: %lu", &lost) == 1
             || sscanf(line, "dropped events: %lu", &lost) == 1) && lost) {
            errno = ENOBUFS;
            fail("trace lost events");
        }
    }
    if (ferror(file) || fclose(file))
        fail(path);
    printf("# END %s\n", path);
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..1");
    if (mount("tracefs", "/sys/kernel/tracing", "tracefs", 0, NULL))
        fail("mount tracefs");
    control("tracing_on", "0");
    control("current_tracer", "nop");
    control("buffer_size_kb", "1024");
    /* GCC inlines user_mem_abort; use the common externally visible root. */
    control("set_graph_function", "kvm_handle_guest_abort");
    control("max_graph_depth", "6");
    control("options/funcgraph-tail", "1");
    control("current_tracer", "function_graph");
    control("trace", "");

    int output = open("/tmp/demand-paging.log", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (output < 0)
        fail("open test output");
    control("tracing_on", "1");
    pid_t child = fork();
    if (child < 0)
        fail("fork");
    if (!child) {
        if (dup2(output, 1) < 0 || dup2(output, 2) < 0)
            _exit(127);
        close(output);
        /* kselftest reads its own ELF using argv[0] to populate guest code. */
        execl("/tests/demand_paging_test", "/tests/demand_paging_test", "-m", "3",
              "-b", "256K", (char *)NULL);
        _exit(127);
    }
    close(output);
    int status;
    while (waitpid(child, &status, 0) < 0) {
        if (errno != EINTR)
            fail("waitpid");
    }
    control("tracing_on", "0");
    report("/tmp/demand-paging.log");
    for (unsigned cpu = 0; cpu < 4; ++cpu) {
        char path[256];
        snprintf(path, sizeof(path), TRACE "per_cpu/cpu%u/stats", cpu);
        report(path);
    }
    report(TRACE "trace");
    control("current_tracer", "nop");
    if (!traced_faults) {
        errno = ENODATA;
        fail("empty fault trace");
    }
    int rc = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    printf("%s 1 one-mode demand-paging diagnostic (original exit %d)\n",
           rc ? "not ok" : "ok", rc);
    return rc ? 1 : 0;
}
