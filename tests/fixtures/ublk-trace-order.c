/* Independent tracefs observer for the original fio/seq_io.bt workload.
 * Diagnostic only: records request issue/completion order without imposing
 * ordering at depth 16. Original fio flags and bpftrace source are unchanged.
 * Exact sector coverage and trace loss are checked; no persistence claim. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <sys/wait.h>
#include <unistd.h>

enum { SECTORS = 512 * 1024 * 2 };
#ifndef TRACE_VARIANTS
#define TRACE_VARIANTS 2
#endif
static const char *tracing = "/sys/kernel/tracing";

static int control(const char *name, const char *value)
{
    char path[256];
    snprintf(path, sizeof(path), "%s/%s", tracing, name);
    int fd = open(path, O_WRONLY | O_TRUNC | O_CLOEXEC);
    if (fd < 0) { perror(path); return 1; }
    ssize_t n = write(fd, value, strlen(value));
    int bad = n != (ssize_t)strlen(value);
    if (bad) perror(path);
    return close(fd) || bad;
}

static int wait_child(pid_t child)
{
    int status;
    while (waitpid(child, &status, 0) < 0)
        if (errno != EINTR) return 255;
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}

static pid_t launch(char *const argv[], const char *output)
{
    pid_t pid = fork();
    if (!pid) {
        int fd = open(output, O_CREAT | O_EXCL | O_WRONLY, 0600);
        if (fd < 0 || dup2(fd, 1) < 0 || dup2(fd, 2) < 0) _exit(126);
        close(fd);
        execvp(argv[0], argv);
        _exit(127);
    }
    return pid;
}

static int inspect_trace(unsigned maj, unsigned min)
{
    char path[256], line[1024];
    unsigned char *seen = calloc(SECTORS, 1);
    unsigned long events = 0, sectors = 0, bad_order = 0, errors = 0;
    unsigned long issues = 0, issue_bad_order = 0, issue_next = 0;
    unsigned long next = 0, prev_sector = 0;
    int prev_cpu = -1;
    unsigned cpu_mask = 0;
    int bad = !seen;
    snprintf(path, sizeof(path), "%s/trace", tracing);
    FILE *file = fopen(path, "r");
    if (!file || !seen) { if (file) fclose(file); free(seen); return 1; }
    while (fgets(line, sizeof(line), file)) {
        char *event = strstr(line, "block_rq_complete:");
        if (!event) {
            event = strstr(line, "block_rq_issue:");
            if (!event) continue;
            unsigned a, b, bytes, length;
            unsigned long sector;
            char rw[16];
            if (sscanf(event, "block_rq_issue: %u,%u %15s %u () %lu + %u",
                       &a, &b, rw, &bytes, &sector, &length) != 6) { ++errors; continue; }
            if (a != maj || b != min || rw[0] != 'W') { ++errors; continue; }
            if (sector != issue_next) ++issue_bad_order;
            issue_next = sector + length;
            ++issues;
            continue;
        }
        unsigned a, b, length;
        unsigned long sector;
        char rw[16];
        char *cpu_start = strchr(line, '[');
        int cpu = -1;
        if (cpu_start) sscanf(cpu_start, "[%d]", &cpu);
        if (sscanf(event, "block_rq_complete: %u,%u %15s () %lu + %u",
                   &a, &b, rw, &sector, &length) != 5) { ++errors; continue; }
        if (a != maj || b != min || rw[0] != 'W' || cpu < 0 || cpu >= 32) {
            ++errors;
            continue;
        }
        cpu_mask |= 1u << cpu;
        if (sector != next) {
            if (bad_order < 16)
                printf("# trace-order event=%lu prev=%lu prev_cpu=%d expected=%lu current=%lu count=%u cpu=%d\n",
                       events, prev_sector, prev_cpu, next, sector, length, cpu);
            ++bad_order;
        }
        if (!strstr(event, "[0]") || !length || sector > SECTORS || length > SECTORS - sector) {
            ++errors;
        } else {
            for (unsigned long i = sector; i < sector + length; ++i) {
                if (seen[i]) ++errors;
                seen[i] = 1;
            }
            sectors += length;
        }
        next = sector + length;
        prev_sector = sector;
        prev_cpu = cpu;
        ++events;
    }
    bad |= ferror(file);
    fclose(file);
    unsigned missing = 0;
    for (unsigned i = 0; i < SECTORS; ++i) missing += !seen[i];
    free(seen);
    unsigned long lost = 0;
    for (unsigned cpu = 0; cpu < 32; ++cpu) {
        snprintf(path, sizeof(path), "%s/per_cpu/cpu%u/stats", tracing, cpu);
        file = fopen(path, "r");
        if (!file) continue;
        while (fgets(line, sizeof(line), file)) {
            unsigned long n;
            if (sscanf(line, "overrun: %lu", &n) == 1
                || sscanf(line, "commit overrun: %lu", &n) == 1
                || sscanf(line, "dropped events: %lu", &n) == 1) lost += n;
        }
        bad |= ferror(file);
        fclose(file);
    }
    printf("# independent trace events=%lu sectors=%lu/%u missing=%u errors=%lu lost=%lu order_breaks=%lu cpus=%x issues=%lu issue_order_breaks=%lu\n",
           events, sectors, SECTORS, missing, errors, lost, bad_order, cpu_mask, issues, issue_bad_order);
    return bad || missing || errors || lost || sectors != SECTORS || !issues;
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("TAP version 13\n1..%d\n", TRACE_VARIANTS);
    if (chdir("/tests/ublk") || system("modprobe ublk_drv")) return 2;
    unsigned failed = 0;
    for (unsigned case_id = 1; case_id <= TRACE_VARIANTS; ++case_id) {
        unsigned queues = case_id == 3 ? 0 : case_id == 4 ? 2 : case_id;
        int pinned = case_id == 4;
        char command[128], device_path[64], dev[32], filter[64];
        char bpf_log[80], fio_log[80];
        if (queues) snprintf(command, sizeof(command), "./kublk add -t null -q %u", queues);
        else snprintf(command, sizeof(command), "./kublk add -t null");
        FILE *pipe = popen(command, "r");
        unsigned device = 0, actual_queues = 0;
        int found = 0;
        if (!pipe) return 2;
        char line[1024];
        while (fgets(line, sizeof(line), pipe)) {
            printf("# device-setup %s", line);
            char *p = strstr(line, "dev id ");
            if (p && sscanf(p, "dev id %u", &device) == 1) ++found;
            p = strstr(line, "nr_hw_queues ");
            if (p) sscanf(p, "nr_hw_queues %u", &actual_queues);
        }
        if (pclose(pipe) || found != 1 || system("udevadm settle")) return 2;
        if (actual_queues != (queues ? queues : 2)) return 2;
        snprintf(device_path, sizeof(device_path), "/dev/ublkb%u", device);
        struct stat st;
        if (stat(device_path, &st)) return 2;
        unsigned maj = major(st.st_rdev), min = minor(st.st_rdev);
        snprintf(dev, sizeof(dev), "%u", (maj << 20) | min);
        snprintf(filter, sizeof(filter), "dev == %s", dev);
        int bad = control("tracing_on", "0") || control("events/enable", "0")
            || control("trace_clock", "global") || control("buffer_size_kb", "32768")
            || control("trace", "") || control("events/block/block_rq_complete/filter", filter)
            || control("events/block/block_rq_issue/filter", filter)
            || control("events/block/block_rq_complete/enable", "1")
            || control("events/block/block_rq_issue/enable", "1");
        if (bad) return 2;
        snprintf(bpf_log, sizeof(bpf_log), "/tmp/ublk-trace-case%u.bpf.log", case_id);
        snprintf(fio_log, sizeof(fio_log), "/tmp/ublk-trace-case%u.fio.log", case_id);
        char *bpf[] = {"bpftrace", "trace/seq_io.bt", dev, "W", "1", NULL};
        pid_t tracer = launch(bpf, bpf_log);
        if (tracer < 0) return 2;
        sleep(2); /* Same readiness delay as the unchanged upstream test. */
        if (kill(tracer, 0) || control("tracing_on", "1")) return 2;
        char filename[96];
        snprintf(filename, sizeof(filename), "--filename=%s", device_path);
        char *fio[] = {"taskset", "-c", "0", "fio", "--name=write_seq", filename, "--ioengine=libaio",
                       "--iodepth=16", "--rw=write", "--size=512M", "--direct=1", "--bs=4k", NULL};
        pid_t writer = launch(fio + (pinned ? 0 : 3), fio_log);
        if (writer < 0) return 2;
        int fio_status = wait_child(writer);
        bad |= control("tracing_on", "0");
        if (kill(tracer, SIGTERM)) bad = 1;
        int tracer_status = wait_child(tracer);
        unsigned bpf_failures = 0;
        int attached = 0, finished = 0;
        FILE *log = fopen(bpf_log, "r");
        if (!log) return 2;
        while (fgets(line, sizeof(line), log)) {
            attached |= strstr(line, "Attaching ") != NULL;
            finished |= strstr(line, "@ios:") != NULL;
            if (strstr(line, "io_out_of_order")) ++bpf_failures;
            if (bpf_failures <= 16) printf("# original-observer %s", line);
        }
        bad |= ferror(log);
        fclose(log);
        printf("# queues=%u actual_queues=%u pinned=%d fio_status=%d tracer_status=%d original_order_breaks=%u\n",
               queues, actual_queues, pinned, fio_status, tracer_status, bpf_failures);
        bad |= inspect_trace(maj, min);
        bad |= fio_status != 0 || tracer_status != 0 || !attached || !finished;
        bad |= control("events/enable", "0");
        snprintf(command, sizeof(command), "./kublk del -n %u >/dev/null", device);
        bad |= system(command) != 0;
        printf("%s %u q%u pinned=%d independent trace coverage and unchanged fio success\n",
               bad ? "not ok" : "ok", case_id, queues, pinned);
        failed += !!bad;
    }
    return !!failed;
}
