/* Bounded diagnostic wrapper, not a kernel patch or original-test waiver.
 * Frozen ARM64 BTF: memcg_stock_pcp nr_pages[7] at 1, cached[7] at 8;
 * mem_cgroup.css at 0. Revalidate these layouts before using another profile. */
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
static void fail(const char *name)
{
    printf("not ok 1 trace setup/capture: %s: %s\n", name, strerror(errno));
    exit(1);
}

static void control(const char *name, const char *text, int append)
{
    char path[256];
    if (snprintf(path, sizeof(path), TRACE "%s", name) >= (int)sizeof(path))
        fail("path");
    int fd = open(path, O_WRONLY | (append ? O_APPEND : O_TRUNC));
    if (fd < 0)
        fail(path);
    size_t n = strlen(text);
    if (write(fd, text, n) != (ssize_t)n || close(fd))
        fail(path);
}

static void report(const char *name)
{
    char path[256], line[4096];
    if (snprintf(path, sizeof(path), TRACE "%s", name) >= (int)sizeof(path))
        fail("path");
    FILE *file = fopen(path, "r");
    if (!file)
        fail(path);
    size_t bytes = 0;
    while (fgets(line, sizeof(line), file)) {
        if ((bytes += strlen(line)) > 4 * 1024 * 1024)
            fail("trace exceeds 4 MiB bound");
        printf("# %s: %s", name, line);
        unsigned long lost;
        if ((sscanf(line, "overrun: %lu", &lost) == 1 ||
             sscanf(line, "commit overrun: %lu", &lost) == 1 ||
             sscanf(line, "dropped events: %lu", &lost) == 1) && lost) {
            errno = ENOBUFS;
            fail("lost events");
        }
    }
    if (ferror(file) || fclose(file))
        fail(path);
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..1");
    if (mount("tracefs", "/sys/kernel/tracing", "tracefs", 0, NULL) && errno != EBUSY)
        fail("mount tracefs");
    control("tracing_on", "0", 0);
    control("current_tracer", "nop", 0);
    control("buffer_size_kb", "1024", 0);
    control("kprobe_events", "p:memcg_diag/drain drain_stock stock=$arg1:x64 index=$arg2:u32 pages=+1($arg1):u8[7] owners=+8($arg1):x64[7]\n", 1);
    control("kprobe_events", "p:memcg_diag/offline mem_cgroup_css_offline css=$arg1:x64\n", 1);
    control("kprobe_events", "p:memcg_diag/released mem_cgroup_css_released css=$arg1:x64\n", 1);
    control("events/memcg_diag/enable", "1", 0);
    control("trace", "", 0);
    control("tracing_on", "1", 0);
    int pipes[2];
    if (pipe(pipes))
        fail("pipe");
    pid_t child = fork();
    if (child < 0)
        fail("fork");
    if (!child) {
        close(pipes[0]);
        if (dup2(pipes[1], 1) < 0)
            _exit(126);
        close(pipes[1]);
        execl("/tests/stock-original", "/tests/stock-original", (char *)NULL);
        _exit(127);
    }
    close(pipes[1]);
    FILE *input = fdopen(pipes[0], "r");
    if (!input)
        fail("fdopen");
    char line[1024];
    int completed = 0;
    while (fgets(line, sizeof(line), input)) {
        printf("# observation: %s", line);
        control("trace_marker", line, 0);
        if (strstr(line, "observation_completed=1 "))
            completed = 1;
    }
    if (ferror(input) || fclose(input))
        fail("child output");
    int status;
    while (waitpid(child, &status, 0) < 0)
        if (errno != EINTR)
            fail("waitpid");
    control("tracing_on", "0", 0);
    report("trace");
    long cpus = sysconf(_SC_NPROCESSORS_CONF);
    for (long cpu = 0; cpu < cpus; cpu++) {
        char name[80];
        snprintf(name, sizeof(name), "per_cpu/cpu%ld/stats", cpu);
        report(name);
    }
    control("events/memcg_diag/enable", "0", 0);
    control("kprobe_events", "-:memcg_diag/drain\n", 1);
    control("kprobe_events", "-:memcg_diag/offline\n", 1);
    control("kprobe_events", "-:memcg_diag/released\n", 1);
    int ok = completed && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    printf("%s 1 complete trace observation, not original-test acceptance\n", ok ? "ok" : "not ok");
    return !ok;
}
