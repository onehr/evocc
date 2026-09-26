/* Diagnostic: compare the frozen one-batch-per-CPU tolerance with the actual
 * seven-slot charge cache. Does not change upstream assertions or the kernel. */
#define _GNU_SOURCE
#include <errno.h>
#include <limits.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/mount.h>
#include <unistd.h>
#include "cgroup_util.h"

static int charge(const char *group, void *arg)
{
    (void)group;
    cpu_set_t one;
    CPU_ZERO(&one);
    CPU_SET(*(int *)arg, &one);
    if (sched_setaffinity(0, sizeof(one), &one))
        return 1;
    long page = sysconf(_SC_PAGESIZE);
    volatile char *p = mmap(NULL, (size_t)page, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED)
        return 1;
    p[0] = 1;
    p[page - 1] = 2;
    return munmap((void *)p, (size_t)page) != 0;
}

static long field(const char *stats, const char *key)
{
    size_t size = strlen(key);
    while (stats) {
        if (!strncmp(stats, key, size) && stats[size] == ' ')
            return strtol(stats + size + 1, NULL, 10);
        stats = strchr(stats, '\n');
        if (stats)
            stats++;
    }
    return -1;
}

static long snapshot(const char *parent, const char *phase, long bound)
{
    char stats[16384];
    if (cg_read(parent, "memory.stat", stats, sizeof(stats)))
        return LONG_MIN;
    long current = cg_read_long(parent, "memory.current");
    long a = field(stats, "anon"), f = field(stats, "file");
    long k = field(stats, "kernel"), s = field(stats, "sock");
    if (current < 0 || a < 0 || f < 0 || k < 0 || s < 0)
        return LONG_MIN;
    long delta = current - a - f - k - s;
    printf("# phase=%s current=%ld sum=%ld delta=%ld old_margin=%ld seven_slots=%ld\n",
           phase, current, a + f + k + s, delta, bound, bound * 7);
    return delta;
}

int main(void)
{
    char root[PATH_MAX];
    char *groups[CPU_SETSIZE * 8] = {0};
    size_t count = 0;
    cpu_set_t cpus;
    int result = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..1");
    if (sched_getaffinity(0, sizeof(cpus), &cpus))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_stock_capacity");
    if (!parent)
        return 1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    long bound = 4096L * 64 * sysconf(_SC_NPROCESSORS_ONLN);
    printf("# cpus=%d page=%ld original_margin=%ld\n", CPU_COUNT(&cpus), sysconf(_SC_PAGESIZE), bound);
    for (int cpu = 0; cpu < CPU_SETSIZE; cpu++) {
        if (!CPU_ISSET(cpu, &cpus))
            continue;
        for (int slot = 0; slot < 8; slot++) {
            char *child = cg_name_indexed(parent, "child", (int)count);
            if (!child)
                goto cleanup;
            groups[count++] = child;
            if (cg_create(child) || cg_run(child, charge, &cpu))
                goto cleanup;
        }
    }
    int exceeded = 0;
    for (int i = 0; i < 4; i++) {
        long delta = snapshot(parent, "charged", bound);
        if (delta == LONG_MIN)
            goto cleanup;
        exceeded |= delta >= bound;
        sleep(1);
    }
    /* Explicit diagnostic reclaim, not an added prerequisite for upstream. */
    errno = 0;
    int reclaim = cg_write(parent, "memory.reclaim", "1");
    printf("# reclaim_return=%d errno=%d\n", reclaim, errno);
    for (int i = 0; i < 3; i++) {
        if (snapshot(parent, "after-reclaim", bound) == LONG_MIN)
            goto cleanup;
        sleep(1);
    }
    result = !exceeded;
cleanup:
    for (size_t i = 0; i < count; i++) {
        if (cg_destroy(groups[i]))
            result = 1;
        free(groups[i]);
    }
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    printf("%s 1 reproduce charge-cache difference beyond frozen tolerance\n", result ? "not ok" : "ok");
    return result;
}
