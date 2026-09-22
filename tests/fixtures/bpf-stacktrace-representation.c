/* Observe the original stacktrace maps; this is not a replacement PASS. */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/wait.h>
#include <unistd.h>
#include <bpf/bpf.h>
#include <bpf/libbpf.h>
#include "test_stacktrace_build_id.skel.h"

static int tracing(const char *name, const char *value)
{
    char path[256];
    snprintf(path, sizeof(path), "/sys/kernel/tracing/%s", name);
    FILE *file = fopen(path, "w");
    if (!file)
        return -1;
    int failed = fputs(value, file) < 0;
    return fclose(file) || failed;
}

static int original(char *const argv[])
{
    pid_t pid = fork();
    if (pid < 0)
        return -1;
    if (!pid) {
        if (chroot("/scratch") || chdir("/tests/bpf"))
            _exit(125);
        execv(argv[0], argv);
        _exit(127);
    }
    int status;
    if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status))
        return -1;
    return WEXITSTATUS(status);
}

int main(void)
{
    if ((mount("debugfs", "/sys/kernel/debug", "debugfs", 0, NULL) && errno != EBUSY)
        || (mount("tracefs", "/sys/kernel/tracing", "tracefs", 0, NULL) && errno != EBUSY)
        || mount("devtmpfs", "/scratch/dev", "devtmpfs", 0, NULL)) {
        perror("diagnostic mounts");
        return 1;
    }
    struct test_stacktrace_build_id *skel = test_stacktrace_build_id__open_and_load();
    if (!skel || test_stacktrace_build_id__attach(skel))
        return 2;
    if (tracing("set_ftrace_filter", "stack_map_get_build_id_offset\n")
        || tracing("trace_options", "irq-info\n")
        || tracing("current_tracer", "function\n") || tracing("tracing_on", "1\n"))
        return 7;
    char *dd[] = {"/usr/bin/dd", "if=/dev/urandom", "of=/dev/zero", "count=4", NULL};
    char *reader[] = {"./urandom_read", NULL};
    if (original(dd) || original(reader))
        return 3;
    if (tracing("tracing_on", "0\n"))
        return 8;
    FILE *trace = fopen("/sys/kernel/tracing/trace", "r");
    if (!trace)
        return 9;
    char line[1024];
    while (fgets(line, sizeof(line), trace))
        fputs(line, stdout);
    int trace_failed = ferror(trace);
    if (fclose(trace) || trace_failed)
        return 10;
    uint32_t zero = 0, one = 1, key, previous, *cursor = NULL;
    if (bpf_map_update_elem(bpf_map__fd(skel->maps.control_map), &zero, &one, 0))
        return 4;
    unsigned keys = 0, mismatches = 0;
    struct bpf_stack_build_id stored[127], fresh[127];
    while (!bpf_map_get_next_key(bpf_map__fd(skel->maps.stackmap), cursor, &key)) {
        if (bpf_map_lookup_elem(bpf_map__fd(skel->maps.stackmap), &key, stored)
            || bpf_map_lookup_elem(bpf_map__fd(skel->maps.stack_amap), &key, fresh))
            return 5;
        keys++;
        for (unsigned i = 0; i < 127; ++i) {
            if (!memcmp(&stored[i], &fresh[i], sizeof(stored[i])))
                continue;
            mismatches++;
            if (mismatches > 12)
                continue;
            printf("key=%u frame=%u stored_status=%u fresh_status=%u "
                   "stored_offset=%" PRIx64 " fresh_offset=%" PRIx64 "\n",
                   key, i, stored[i].status, fresh[i].status,
                   (uint64_t)stored[i].offset, (uint64_t)fresh[i].offset);
            printf("stored_build_id=");
            for (unsigned j = 0; j < sizeof(stored[i].build_id); ++j)
                printf("%02x", stored[i].build_id[j]);
            printf(" fresh_build_id=");
            for (unsigned j = 0; j < sizeof(fresh[i].build_id); ++j)
                printf("%02x", fresh[i].build_id[j]);
            putchar('\n');
        }
        previous = key;
        cursor = &previous;
    }
    printf("observation_complete keys=%u differing_frames=%u\n", keys, mismatches);
    test_stacktrace_build_id__destroy(skel);
    return keys ? 0 : 6;
}
