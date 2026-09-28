/* Diagnostic reduction, not the unchanged stress_04 acceptance program.
 * A single null/independent loop device removes backing-file sharing. Stop its
 * daemon while buffered libaio/io_uring clients fill a small queue, then kill it.
 * Every client and deletion must finish: expected I/O errors are not failures,
 * but an unfinished client, failed deletion or surviving dirty I/O is.
 * Reference-built userspace isolates the kernel from EvoCC userspace output. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#ifndef UBLK_WARM_LOOP
#define UBLK_WARM_LOOP 0
#endif
#ifndef UBLK_RUNNING_KILL
#define UBLK_RUNNING_KILL 0
#endif
#ifndef UBLK_BEFORE_KILL
#define UBLK_BEFORE_KILL(ready) ((void)(ready), 0)
#endif

static double now(void)
{
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t))
        abort();
    return t.tv_sec + t.tv_nsec / 1e9;
}

static int finish(pid_t child, unsigned seconds, int *status)
{
    double end = now() + seconds;
    while (now() < end) {
        pid_t got = waitpid(child, status, WNOHANG);
        if (got == child)
            return 0;
        if (got < 0 && errno != EINTR)
            return 1;
        usleep(10000);
    }
    /* Reap only if it has actually exited. A D-state worker is retained for
     * the surrounding observer; do not block here and lose that evidence. */
    kill(child, SIGKILL);
    waitpid(child, status, WNOHANG);
    return 1;
}

static pid_t fio(unsigned device, int uring)
{
    pid_t child = fork();
    if (child != 0)
        return child;
    char log[128];
    snprintf(log, sizeof(log), "/tmp/k1-ublk-fio-%u-%u.log", device, (unsigned)uring);
    int output = open(log, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (output < 0 || dup2(output, 1) < 0 || dup2(output, 2) < 0)
        _exit(120);
    if (output > 2)
        close(output);
    char path[64];
    snprintf(path, sizeof(path), "--filename=/dev/ublkb%u", device);
    if (uring)
        execlp("fio", "fio", "--name=batchjob", path, "--ioengine=io_uring",
               "--rw=randrw", "--norandommap", "--iodepth=256", "--size=64M",
               "--numjobs=4", "--runtime=20", "--time_based",
               "--iodepth_batch_submit=32", "--iodepth_batch_complete_min=32",
               "--force_async=7", (char *)0);
    else
        execlp("fio", "fio", "--name=job1", path, "--ioengine=libaio",
               "--rw=randrw", "--norandommap", "--iodepth=256", "--size=64M",
               "--numjobs=4", "--runtime=20", "--time_based", (char *)0);
    _exit(121);
}

static int queued(unsigned device)
{
    char path[128];
    snprintf(path, sizeof(path), "/sys/block/ublkb%u/inflight", device);
    double end = now() + 5;
    while (now() < end) {
        unsigned long reads = 0, writes = 0;
        FILE *file = fopen(path, "r");
        if (!file)
            return 0;
        int fields = fscanf(file, "%lu %lu", &reads, &writes);
        fclose(file);
        if (fields != 2)
            return 0;
        if (reads || writes) {
            printf("# queued reads=%lu writes=%lu\n", reads, writes);
            return 1;
        }
        usleep(10000);
    }
    return 0;
}

static unsigned long completed_count(unsigned device)
{
    char path[128];
    snprintf(path, sizeof(path), "/sys/block/ublkb%u/stat", device);
    unsigned long fields[5];
    FILE *file = fopen(path, "r");
    if (!file)
        return 0;
    int count = fscanf(file, "%lu %lu %lu %lu %lu", &fields[0], &fields[1],
                       &fields[2], &fields[3], &fields[4]);
    fclose(file);
    return count == 5 ? fields[0] + fields[4] : 0;
}

static int completed(unsigned device, unsigned long before)
{
    double end = now() + 5;
    while (now() < end) {
        unsigned long count = completed_count(device);
        if (count >= before + 64) {
            printf("# completed before=%lu now=%lu\n", before, count);
            return 1;
        }
        usleep(10000);
    }
    return 0;
}

static void client_log(unsigned device, unsigned client)
{
    char path[128], bytes[8192];
    snprintf(path, sizeof(path), "/tmp/k1-ublk-fio-%u-%u.log", device, client);
    int fd = open(path, O_RDONLY);
    if (fd < 0)
        return;
    ssize_t count = read(fd, bytes, sizeof(bytes));
    if (count > 0) {
        printf("# client=%u output (%zd bytes maximum 8192)\n", client, count);
        if (write(1, bytes, (size_t)count) != count)
            puts("# client log output error");
        puts("");
    }
    close(fd);
}

static pid_t flush_device(unsigned device)
{
    char path[128];
    snprintf(path, sizeof(path), "/dev/ublkb%u", device);
    int fd = open(path, O_WRONLY);
    if (fd < 0)
        return -1;
    pid_t child = fork();
    if (!child) {
        int error = fdatasync(fd) ? errno : 0;
        printf("# explicit writeback errno=%d\n", error);
        close(fd);
        _exit(error ? 4 : 0);
    }
    close(fd);
    return child;
}

static int probe(const char *mode)
{
    char command[256], line[512];
    unsigned device = 0;
    int daemon = 0, found = 0;
    snprintf(command, sizeof(command), "/tests/ublk/kublk add -t %s -q 4 -d 16 %s %s",
             UBLK_WARM_LOOP ? "loop" : "null", mode,
             UBLK_WARM_LOOP ? "/tmp/k1-ublk-cancel-backing" : "");
    FILE *pipe = popen(command, "r");
    if (!pipe)
        return 1;
    while (fgets(line, sizeof(line), pipe)) {
        printf("# %s", line);
        char *start = strstr(line, "dev id ");
        if (start && sscanf(start, "dev id %u", &device) == 1)
            ++found;
        start = strstr(line, "pid ");
        if (start)
            sscanf(start, "pid %d", &daemon);
    }
    int status = pclose(pipe);
    if (status || found != 1 || daemon <= 1 || system("udevadm settle"))
        return 1;
    printf("# created device=%u daemon=%d mode=%s\n", device, daemon, mode);
    unsigned long before = completed_count(device);
    if (!UBLK_WARM_LOOP && kill(daemon, SIGSTOP))
        return 1;
    pid_t children[] = {fio(device, 0), fio(device, 1), -1};
    int bad = children[0] <= 0 || children[1] <= 0;
    if (UBLK_WARM_LOOP) {
        /* Unlike the cold-null control, exercise the server's actual backing
         * I/O and registered-buffer path before interrupting this daemon. */
        bad |= !completed(device, before);
        usleep(2000000);
        if (!UBLK_RUNNING_KILL && kill(daemon, SIGSTOP))
            return 1;
        /* A 64 MiB buffered working set can fit entirely in cache. Request
         * real writeback so a zero inflight count cannot be mistaken for a
         * cancellation failure merely because dirty expiry has not elapsed. */
        children[2] = flush_device(device);
        bad |= children[2] <= 0;
    }
    /* The running variant retains completion-versus-exit concurrency; the
     * original cold/stopped controls deliberately remove that race.
     * Cancellation without a queued request would be a vacuous observation.
     * Keep the owned daemon stopped until sysfs confirms actual outstanding
     * I/O, and record a setup failure rather than PASS if no request arrives. */
    bad |= !queued(device);
    if (bad)
        puts("# could not establish outstanding I/O: setup failure");
    printf("# cancellation control: daemon=%d stopped=%d\n", daemon,
           !UBLK_WARM_LOOP || !UBLK_RUNNING_KILL);
    bad |= UBLK_BEFORE_KILL(!bad);
    if (kill(daemon, SIGKILL))
        return 1;
    printf("# killed daemon device=%u\n", device);
    for (unsigned i = 0; i < (UBLK_WARM_LOOP ? 3u : 2u); ++i) {
        status = -1;
        int timedout = children[i] <= 0 || finish(children[i], 8, &status);
        printf("# client=%u timedout=%d wait_status=%d\n", i, timedout, status);
        /* EIO caused by daemon death is expected. Missing executable and
         * failed supervisor setup are not valid cancellation observations. */
        bad |= timedout || !WIFEXITED(status)
            || (WEXITSTATUS(status) != 0 && WEXITSTATUS(status) != 4);
    }
    if (bad)
        for (unsigned i = 0; i < 2; ++i)
            client_log(device, i);
    snprintf(command, sizeof(command), "timeout 8 /tests/ublk/kublk del -n %u", device);
    status = system(command);
    printf("# deletion device=%u wait_status=%d\n", device, status);
    return bad || status != 0;
}

int main(void)
{
    const char *modes[] = {"-z --no_ublk_fixed_fd", "--auto_zc",
                          "--nthreads 8 --per_io_tasks",
                          "-z --auto_zc --auto_zc_fallback"};
    /* The installed server implements the fourth mode only for null, exactly
     * as used by upstream stress_04. Do not label its loop setup refusal as a
     * kernel cancellation result. */
    unsigned cases = UBLK_WARM_LOOP ? 3u : 4u;
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("TAP version 13\n1..%u\n", cases);
    if (system("modprobe ublk_drv"))
        return 2;
    if (UBLK_WARM_LOOP) {
        int fd = open("/tmp/k1-ublk-cancel-backing", O_RDWR | O_CREAT | O_EXCL, 0600);
        if (fd < 0 || ftruncate(fd, 64 * 1024 * 1024) || close(fd))
            return 2;
        puts(UBLK_RUNNING_KILL
             ? "# variant: independent loop backing, kill running daemon during queued I/O"
             : "# variant: independent loop backing, actual I/O before daemon stop");
    }
    for (unsigned i = 0; i < cases; ++i) {
        int bad = probe(modes[i]);
        printf("%s %u single-device cancellation %s\n", bad ? "not ok" : "ok",
               i + 1, modes[i]);
        if (bad) {
            puts("# stop after first failure; remaining cases NOT RUN");
            return 1;
        }
    }
    return 0;
}
