/* Diagnostic only: retain the frozen workload, observe after its immediate
 * accounting check, and do not replace any upstream expected result. */
#define main original_kmem_main
#include "test_kmem.c"
#undef main
#include <sys/mount.h>

static long stat_field(const char *text, const char *key)
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
    setvbuf(stdout, NULL, _IONBF, 0);
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_deletion_diagnostic");
    if (!parent)
        return 1;
    int result = 1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    if (cg_run_in_subcgroups(parent, alloc_kmem_smp, NULL, 100))
        goto cleanup;
    for (int second = 0; second <= 20; second++) {
        char stats[16384];
        long current = cg_read_long(parent, "memory.current");
        if (cg_read(parent, "memory.stat", stats, sizeof(stats)))
            goto cleanup;
        long anon = stat_field(stats, "anon"), file = stat_field(stats, "file");
        long kernel = stat_field(stats, "kernel"), sock = stat_field(stats, "sock");
        long dead = cg_read_key_long(parent, "cgroup.stat", "nr_dying_descendants ");
        if (current < 0 || anon < 0 || file < 0 || kernel < 0 || sock < 0 || dead < 0)
            goto cleanup;
        long delta = current - anon - file - kernel - sock;
        printf("second=%d current=%ld anon=%ld file=%ld kernel=%ld sock=%ld "
               "slab=%ld percpu=%ld delta=%ld margin=%d dying=%ld\n",
               second, current, anon, file, kernel, sock,
               stat_field(stats, "slab"), stat_field(stats, "percpu"),
               delta, MAX_VMSTAT_ERROR, dead);
        if (second != 20)
            sleep(1);
    }
    /* Observation completion only, not a passing accounting assertion. */
    result = 0;
cleanup:
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    return result;
}
