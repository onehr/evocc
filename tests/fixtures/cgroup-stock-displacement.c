/* Diagnostic, not an upstream assertion replacement. Reuse the frozen kmem
 * workload, then compare idle/root charging with per-CPU distinct-memcg charging.
 * Exit zero means observations completed, not that the original test passed. */
#define main original_kmem_main
#include "test_kmem.c"
#undef main
#include <sched.h>
#include <sys/mman.h>
#include <sys/mount.h>

static int charge_one_page(const char *cgroup, void *arg)
{
    (void)cgroup;
    int cpu = *(int *)arg;
    cpu_set_t affinity;
    CPU_ZERO(&affinity);
    CPU_SET(cpu, &affinity);
    if (sched_setaffinity(0, sizeof(affinity), &affinity))
        return 1;
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0)
        return 1;
    volatile char *p = mmap(NULL, (size_t)page, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED)
        return 1;
    p[0] = 1;
    p[page - 1] = 2;
    return munmap((void *)p, (size_t)page) != 0;
}

static int observe(const char *parent, const char *phase, int second)
{
    long dead = cg_read_key_long(parent, "cgroup.stat", "nr_dying_descendants ");
    long current = cg_read_long(parent, "memory.current");
    printf("phase=%s second=%d dying=%ld current=%ld\n", phase, second, dead, current);
    return dead < 0 || current < 0;
}

int main(int argc, char **argv)
{
    (void)argc;
    char root[PATH_MAX];
    char *groups[CPU_SETSIZE * 8] = {0};
    size_t count = 0;
    cpu_set_t cpus;
    int result = 1;
    setvbuf(stdout, NULL, _IONBF, 0);
    if (sched_getaffinity(0, sizeof(cpus), &cpus))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_stock_observation");
    if (!parent)
        return 1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    int threaded = strstr(argv[0], "threaded") != NULL;
    printf("workload=%s cpus=%d stock_slots=7\n",
           threaded ? "100-threaded-kmem-groups" : "30-original-dead-groups",
           CPU_COUNT(&cpus));
    /* Preserve even the frozen helper's ignored arg; don't silently repair it. */
    if (cg_run_in_subcgroups(parent, threaded ? alloc_kmem_smp : alloc_dcache,
                            threaded ? NULL : (void *)100, threaded ? 100 : 30))
        goto cleanup;
    for (int second = 0; second <= 20; second++) {
        if (observe(parent, "idle", second))
            goto cleanup;
        if (second < 20)
            sleep(1);
    }
    /* First exercise the same affinity/map/fork machinery in root, without
     * replacing cached child-memcg identities. It has its own measured phase. */
    for (int cpu = 0; cpu < CPU_SETSIZE; cpu++)
        if (CPU_ISSET(cpu, &cpus))
            for (int slot = 0; slot < 8; slot++)
                if (cg_run(root, charge_one_page, &cpu))
                    goto cleanup;
    for (int second = 0; second <= 3; second++) {
        if (observe(parent, "root-control", second))
            goto cleanup;
        if (second < 3)
            sleep(1);
    }
    /* New groups live outside parent and remain alive until all observations.
     * Eight distinct charges on each CPU can replace its seven stock entries.
     * This is a diagnostic perturbation, never an original-test prerequisite. */
    for (int cpu = 0; cpu < CPU_SETSIZE; cpu++) {
        if (!CPU_ISSET(cpu, &cpus))
            continue;
        for (int slot = 0; slot < 8; slot++) {
            char *child = cg_name_indexed(root, "evocc_stock_control", (int)count);
            if (!child)
                goto cleanup;
            groups[count++] = child;
            if (cg_create(child) || cg_run(child, charge_one_page, &cpu))
                goto cleanup;
        }
    }
    for (int second = 0; second <= 5; second++) {
        if (observe(parent, "distinct-stock", second))
            goto cleanup;
        if (second < 5)
            sleep(1);
    }
    result = 0;
cleanup:
    for (size_t i = 0; i < count; i++) {
        if (cg_destroy(groups[i]))
            result = 1;
        free(groups[i]);
    }
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    printf("observation_completed=%d original_verdict_unchanged=1\n", !result);
    return result;
}
