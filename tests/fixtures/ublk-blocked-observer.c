/* Diagnostic-only init wrapper: observe the unchanged upstream workload before
 * its timeout kills workers. This neither changes the test nor its budget. */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static int diagnostic_usleep(useconds_t delay);
static void diagnostic_sync(void);
#ifdef UBLK_DIAGNOSTIC_PROBE
static int probe_fd;
static int diagnostic_exec(const char *path, const char *arg, ...);
static _Noreturn void poweroff(void);
#define execl diagnostic_exec
#define main selftest_main
#endif
#define usleep diagnostic_usleep
#define sync diagnostic_sync
#include "selftest-init.c"
#undef sync
#undef usleep
#ifdef UBLK_DIAGNOSTIC_PROBE
#undef main
#undef execl

static int diagnostic_exec(const char *path, const char *arg, ...)
{
    (void)arg;
    if (!strcmp(path, "/tests/run_kselftest.sh")) {
        char *argv[] = {"ublk-kill-cancel", NULL};
        return fexecve(probe_fd, argv, environ);
    }
    errno = ENOSYS;
    return -1;
}

int main(void)
{
    probe_fd = open("/probe", O_RDONLY);
    if (probe_fd < 0)
        fatal("open ublk cancellation probe");
    return selftest_main();
}
#endif

static void diagnostic_file(const char *path)
{
    char bytes[4096];
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return;
    ssize_t count = read(fd, bytes, sizeof(bytes));
    if (count > 0) {
        /* cmdline contains NUL separators; this is display only. */
        for (ssize_t i = 0; i < count; ++i)
            if (!bytes[i])
                bytes[i] = ' ';
        dprintf(1, "KSELFTEST_DIAGNOSTIC %s\n", path);
        if (write(1, bytes, (size_t)count) != count)
            dprintf(1, "KSELFTEST_DIAGNOSTIC short output\n");
        dprintf(1, "\n");
    }
    close(fd);
}

static void diagnostic_threads(const char *pid)
{
    char path[512];
    snprintf(path, sizeof(path), "/proc/%s/task", pid);
    DIR *tasks = opendir(path);
    if (!tasks)
        return;
    struct dirent *entry;
    unsigned count = 0;
    while ((entry = readdir(tasks)) && count < 64) {
        if (strspn(entry->d_name, "0123456789") != strlen(entry->d_name))
            continue;
        ++count;
        /* Thread IDs are directly accessible under /proc, even when readdir
         * only reports process leaders. Do not omit io_uring exit workers. */
        const char *fields[] = {"comm", "wchan", "stack"};
        for (unsigned i = 0; i < sizeof(fields) / sizeof(fields[0]); ++i) {
            snprintf(path, sizeof(path), "/proc/%s/%s", entry->d_name, fields[i]);
            diagnostic_file(path);
        }
    }
    if (count == 64)
        dprintf(1, "KSELFTEST_DIAGNOSTIC thread limit reached pid=%s\n", pid);
    closedir(tasks);
}

static void diagnostic_devices(void)
{
    DIR *devices = opendir("/sys/block");
    if (!devices)
        return;
    struct dirent *entry;
    unsigned count = 0;
    while ((entry = readdir(devices)) && count < 32) {
        if (strncmp(entry->d_name, "ublkb", 5))
            continue;
        ++count;
        char path[512];
        const char *fields[] = {"dev", "inflight", "stat", "queue/nr_requests"};
        for (unsigned i = 0; i < sizeof(fields) / sizeof(fields[0]); ++i) {
            snprintf(path, sizeof(path), "/sys/block/%s/%s", entry->d_name, fields[i]);
            diagnostic_file(path);
        }
        snprintf(path, sizeof(path), "/sys/kernel/debug/block/%s/state", entry->d_name);
        diagnostic_file(path);
        snprintf(path, sizeof(path), "/sys/kernel/debug/block/%s/requeue_list", entry->d_name);
        diagnostic_file(path);
        const char *queue_fields[] = {"state", "dispatch", "busy", "tags", "active"};
        /* The unchanged stress workload requests four hardware queues. */
        for (unsigned q = 0; q < 4; ++q)
            for (unsigned i = 0; i < sizeof(queue_fields) / sizeof(queue_fields[0]); ++i) {
                snprintf(path, sizeof(path), "/sys/kernel/debug/block/%s/hctx%u/%s",
                         entry->d_name, q, queue_fields[i]);
                diagnostic_file(path);
            }
    }
    closedir(devices);
}

static void diagnostic_snapshot(const char *phase)
{
    dprintf(1, "\nKSELFTEST_DIAGNOSTIC blocked snapshot %s\n", phase);
    int log = open("/dev/kmsg", O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (log >= 0 && lseek(log, 0, SEEK_END) < 0) {
        close(log);
        log = -1;
    }
    int fd = open("/proc/sysrq-trigger", O_WRONLY | O_CLOEXEC);
    if (fd >= 0) {
        if (write(fd, "w", 1) != 1)
            dprintf(1, "KSELFTEST_DIAGNOSTIC sysrq failed: %s\n", strerror(errno));
        close(fd);
    }
    if (log >= 0) {
        char record[8192];
        for (unsigned i = 0; i < 256; ++i) {
            ssize_t count = read(log, record, sizeof(record));
            if (count < 0 && errno == EPIPE)
                continue;
            if (count <= 0)
                break;
            if (write(1, record, (size_t)count) != count)
                break;
        }
        close(log);
    }
    diagnostic_file("/proc/meminfo");
    diagnostic_file("/proc/pressure/io");
    diagnostic_devices();
    DIR *processes = opendir("/proc");
    if (!processes)
        return;
    struct dirent *entry;
    while ((entry = readdir(processes))) {
        if (!entry->d_name[0] || strspn(entry->d_name, "0123456789") != strlen(entry->d_name))
            continue;
        char path[512], comm[80] = {0};
        snprintf(path, sizeof(path), "/proc/%s/comm", entry->d_name);
        fd = open(path, O_RDONLY | O_CLOEXEC);
        if (fd < 0)
            continue;
        ssize_t count = read(fd, comm, sizeof(comm) - 1);
        close(fd);
        if (count <= 0 || (!strstr(comm, "fio") && !strstr(comm, "ublk")
                          && !strstr(comm, "bash") && !strstr(comm, "timeout")))
            continue;
        dprintf(1, "KSELFTEST_DIAGNOSTIC pid %s %s", entry->d_name, comm);
        snprintf(path, sizeof(path), "/proc/%s/cmdline", entry->d_name);
        diagnostic_file(path);
        snprintf(path, sizeof(path), "/proc/%s/status", entry->d_name);
        diagnostic_file(path);
        snprintf(path, sizeof(path), "/proc/%s/wchan", entry->d_name);
        diagnostic_file(path);
        if (strstr(comm, "ublk"))
            diagnostic_threads(entry->d_name);
        snprintf(path, sizeof(path), "/proc/%s/stack", entry->d_name);
        diagnostic_file(path);
        dprintf(1, "\n");
    }
    closedir(processes);
    dprintf(1, "KSELFTEST_DIAGNOSTIC snapshot complete\n");
}

static int diagnostic_usleep(useconds_t delay)
{
    static double started;
    static unsigned next;
#ifndef UBLK_DIAGNOSTIC_TIMES
#define UBLK_DIAGNOSTIC_TIMES 25, 40, 46, 58
#endif
    static const unsigned times[] = {UBLK_DIAGNOSTIC_TIMES};
    double now = monotonic();
    if (!started)
        started = now;
    if (next < sizeof(times) / sizeof(times[0]) && now - started >= times[next]) {
        ++next;
        diagnostic_snapshot("during unchanged workload");
    }
    return usleep(delay);
}

static void diagnostic_sync(void)
{
    diagnostic_snapshot("before shutdown sync");
    sync();
}
