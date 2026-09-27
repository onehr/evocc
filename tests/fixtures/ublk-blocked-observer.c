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
#define usleep diagnostic_usleep
#define sync diagnostic_sync
#include "selftest-init.c"
#undef sync
#undef usleep

static void diagnostic_file(const char *path)
{
    char bytes[4096];
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return;
    ssize_t count = read(fd, bytes, sizeof(bytes));
    if (count > 0) {
        dprintf(1, "KSELFTEST_DIAGNOSTIC %s\n", path);
        if (write(1, bytes, (size_t)count) != count)
            dprintf(1, "KSELFTEST_DIAGNOSTIC short output\n");
    }
    close(fd);
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
        snprintf(path, sizeof(path), "/proc/%s/wchan", entry->d_name);
        diagnostic_file(path);
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
    static const unsigned times[] = {25, 40, 46, 58};
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
