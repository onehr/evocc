/* Diagnostic reduction of test_percpu_basic, not an upstream replacement.
 * Keep 1000 children and print both accounting formulas from one stat read.
 * Build with the frozen selftest's cgroup_util.c. */
#define _GNU_SOURCE
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/sysinfo.h>
#include <unistd.h>
#include "cgroup_util.h"

static long field(const char *text, const char *key)
{
    size_t length = strlen(key);
    while (*text) {
        if (!strncmp(text, key, length) && text[length] == ' ')
            return strtol(text + length + 1, NULL, 10);
        text = strchr(text, '\n');
        if (!text)
            break;
        text++;
    }
    return -1;
}

int main(void)
{
    char root[PATH_MAX];
    int created = 0, result = 1, valid_snapshots = 0;
    setvbuf(stdout, NULL, _IONBF, 0);
    /* The small diagnostic init does not do the full suite's cgroup mount. */
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL)) {
        perror("mount cgroup2");
        return 1;
    }
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_read_strstr(root, "cgroup.controllers", "memory")) {
        puts("diagnostic unavailable: cgroup v2 memory controller");
        return 4;
    }
    if (cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_percpu_diagnostic");
    if (!parent)
        return 1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    for (; created < 1000; created++) {
        char *child = cg_name_indexed(parent, "child", created);
        if (!child)
            goto cleanup;
        int error = cg_create(child);
        free(child);
        if (error)
            goto cleanup;
    }
    long margin = 4096L * 64 * get_nprocs();
    printf("diagnostic children=%d cpus=%d pagesize=%ld margin=%ld\n",
           created, get_nprocs(), sysconf(_SC_PAGESIZE), margin);
    for (int second = 0; second < 4; second++) {
        char stats[16384];
        long current = cg_read_long(parent, "memory.current");
        if (cg_read(parent, "memory.stat", stats, sizeof(stats)))
            goto cleanup;
        long percpu = field(stats, "percpu"), slab = field(stats, "slab");
        if (current < 0 || percpu < 0 || slab < 0)
            goto cleanup;
        long original_delta = current - percpu;
        long with_slab_delta = current - percpu - slab;
        printf("second=%d current=%ld percpu=%ld slab=%ld "
               "original_delta=%ld with_slab_delta=%ld original_check=%d with_slab_check=%d\n",
               second, current, percpu, slab, original_delta, with_slab_delta,
               labs(original_delta) < margin, labs(with_slab_delta) < margin);
        valid_snapshots++;
        if (second != 3)
            sleep(1);
    }
    /* Exit zero records observations only; it is not a test_kmem PASS. */
    result = valid_snapshots == 4 ? 0 : 1;
cleanup:
    while (created) {
        char *child = cg_name_indexed(parent, "child", --created);
        if (!child || cg_destroy(child))
            result = 1;
        free(child);
    }
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    return result;
}
