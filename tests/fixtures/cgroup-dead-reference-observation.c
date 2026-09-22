/* Diagnostic, not an upstream test replacement. Reuse the frozen workload
 * and observe whether scheduling a live userspace mm on every CPU releases
 * the references left after the original twenty-second observation window. */
#define main original_kmem_main
#include "test_kmem.c"
#undef main
#include <sched.h>
#include <sys/mount.h>

static long observe(const char *parent, const char *phase, int second)
{
    long dead = cg_read_key_long(parent, "cgroup.stat", "nr_dying_descendants ");
    long current = cg_read_long(parent, "memory.current");
    printf("phase=%s second=%d dying=%ld current=%ld\n",
           phase, second, dead, current);
    return dead;
}

int main(void)
{
    char root[PATH_MAX];
    cpu_set_t original;
    setvbuf(stdout, NULL, _IONBF, 0);
    if (sched_getaffinity(0, sizeof(original), &original))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_dead_reference_diagnostic");
    if (!parent)
        return 1;
    int result = 1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    /* Keep even the frozen helper's NULL-argument behavior unchanged. */
    if (cg_run_in_subcgroups(parent, alloc_dcache, (void *)100, 30))
        goto cleanup;
    for (int second = 0; second <= 20; second++) {
        if (observe(parent, "original-wait", second) < 0)
            goto cleanup;
        if (second != 20)
            sleep(1);
    }
    for (int cpu = 0; cpu < CPU_SETSIZE; cpu++) {
        if (!CPU_ISSET(cpu, &original))
            continue;
        cpu_set_t one;
        CPU_ZERO(&one);
        CPU_SET(cpu, &one);
        if (sched_setaffinity(0, sizeof(one), &one))
            goto restore;
        printf("userspace-context cpu=%d actual=%d\n", cpu, sched_getcpu());
        if (observe(parent, "after-cpu", cpu) < 0)
            goto restore;
    }
    if (sched_setaffinity(0, sizeof(original), &original))
        goto cleanup;
    for (int second = 0; second <= 5; second++) {
        if (observe(parent, "after-contexts", second) < 0)
            goto cleanup;
        if (second != 5)
            sleep(1);
    }
    /* Zero means observation completed, not that the upstream test passed. */
    result = 0;
    goto cleanup;
restore:
    if (sched_setaffinity(0, sizeof(original), &original))
        perror("restore affinity");
cleanup:
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    return result;
}
