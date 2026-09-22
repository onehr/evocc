/* Diagnostic companion, not a replacement for upstream remove_on_exec. */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <linux/perf_event.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

static int open_event(const char *name, struct perf_event_attr attr)
{
    errno = 0;
    int fd = syscall(__NR_perf_event_open, &attr, 0, -1, -1,
                     PERF_FLAG_FD_CLOEXEC);
    int error = errno;
    printf("# %s: fd=%d errno=%d (%s) type=%u config=%llu size=%u\n",
           name, fd, error, strerror(error), attr.type,
           (unsigned long long)attr.config, attr.size);
    if (fd >= 0)
        close(fd);
    return fd >= 0;
}

int main(void)
{
    struct perf_event_attr original = {
        .type = PERF_TYPE_HARDWARE,
        .size = sizeof(original),
        .config = PERF_COUNT_HW_INSTRUCTIONS,
        .sample_period = 1000,
        .exclude_kernel = 1,
        .exclude_hv = 1,
        .disabled = 1,
        .inherit = 1,
        .remove_on_exec = 1,
        .sigtrap = 1,
    };
    DIR *devices = opendir("/sys/bus/event_source/devices");
    if (devices) {
        struct dirent *entry;
        while ((entry = readdir(devices)))
            if (entry->d_name[0] != '.')
                printf("# event source: %s\n", entry->d_name);
        closedir(devices);
    }
    puts("TAP version 13\n1..1");
    int supported = open_event("original instruction sampling", original);
    struct perf_event_attr control = original;
    control.sigtrap = 0;
    control.remove_on_exec = 0;
    control.inherit = 0;
    control.sample_period = 0;
    open_event("instruction counting", control);
    control.config = PERF_COUNT_HW_CPU_CYCLES;
    open_event("cycle counting", control);
    control.type = PERF_TYPE_SOFTWARE;
    control.config = PERF_COUNT_SW_TASK_CLOCK;
    open_event("software task clock", control);
    printf("%s 1 original hardware instruction event is available\n",
           supported ? "ok" : "not ok");
    return !supported;
}
