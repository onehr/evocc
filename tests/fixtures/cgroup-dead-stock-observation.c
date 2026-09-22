/* Observe the frozen dead-cgroup workload, then drain per-CPU stocks by
 * hotplug. This is a diagnostic, not an upstream test or a kernel patch. */
#define main original_kmem_main
#include "test_kmem.c"
#undef main
#include <sched.h>
#include <sys/mount.h>

static int pin_cpu(int cpu)
{
    cpu_set_t one;
    CPU_ZERO(&one);
    CPU_SET(cpu, &one);
    if (sched_setaffinity(0, sizeof(one), &one))
        return -1;
    printf("pin requested=%d actual=%d\n", cpu, sched_getcpu());
    return sched_getcpu() == cpu ? 0 : -1;
}

static int observe(const char *parent, const char *phase, int seconds)
{
    for (int second = 0; second <= seconds; second++) {
        long dying = cg_read_key_long(parent, "cgroup.stat",
                                     "nr_dying_descendants ");
        long current = cg_read_long(parent, "memory.current");
        printf("phase=%s second=%d dying=%ld current=%ld\n",
               phase, second, dying, current);
        if (dying < 0 || current < 0)
            return -1;
        if (second != seconds)
            sleep(1);
    }
    return 0;
}

int main(void)
{
    char root[PATH_MAX];
    cpu_set_t original;
    char online[128] = {0};
    bool offline = false;
    int result = 1;
    char *parent = NULL;
    setvbuf(stdout, NULL, _IONBF, 0);
    if (sched_getaffinity(0, sizeof(original), &original) ||
        !CPU_ISSET(0, &original) || !CPU_ISSET(1, &original))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    parent = cg_name(root, "evocc_dead_stock_diagnostic");
    if (!parent || cg_create(parent) ||
        cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    /* Retain the frozen workload's affinity and NULL-argument behavior. */
    if (cg_run_in_subcgroups(parent, alloc_dcache, (void *)100, 30) ||
        observe(parent, "original-wait", 20))
        goto cleanup;
    for (int cpu = 0; cpu < CPU_SETSIZE; cpu++) {
        if (!CPU_ISSET(cpu, &original))
            continue;
        if (pin_cpu(cpu == 0 ? 1 : 0))
            goto cleanup;
        snprintf(online, sizeof(online), "/sys/devices/system/cpu/cpu%d/online", cpu);
        if (write_text(online, "0", 1) != 1)
            goto cleanup;
        offline = true;
        printf("offline cpu=%d; memcg_hotplug_cpu_dead drains its stocks\n", cpu);
        if (observe(parent, "after-cpu-drain", 1))
            goto cleanup;
        if (write_text(online, "1", 1) != 1)
            goto cleanup;
        offline = false;
    }
    if (observe(parent, "after-stock-drain", 5))
        goto cleanup;
    /* Success means completed measurement, not dying==0 or upstream PASS. */
    result = 0;
cleanup:
    if (offline && write_text(online, "1", 1) != 1)
        result = 1;
    if (sched_setaffinity(0, sizeof(original), &original))
        result = 1;
    if (parent && cg_destroy(parent))
        result = 1;
    free(parent);
    return result;
}
