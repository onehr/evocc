/* Diagnostic only. Track charged pages after the frozen dead-cgroup workload;
 * do not change the original test's allocation argument or twenty-second wait. */
#define main original_kmem_main
#include "test_kmem.c"
#undef main
#include <stdint.h>
#include <sys/mount.h>

struct child_pages {
    unsigned long long inode, pages, flags, mappings, first_pfn;
};

int main(void)
{
    char root[PATH_MAX];
    struct child_pages children[30] = {0};
    setvbuf(stdout, NULL, _IONBF, 0);
    if (cg_find_unified_root(root, sizeof(root), NULL) &&
        mount("none", "/sys/fs/cgroup", "cgroup2", 0, NULL))
        return 1;
    if (cg_find_unified_root(root, sizeof(root), NULL) ||
        cg_write(root, "cgroup.subtree_control", "+memory"))
        return 1;
    char *parent = cg_name(root, "evocc_dead_page_diagnostic");
    if (!parent)
        return 1;
    int result = 1;
    FILE *owners = NULL;
    int flags = -1, mappings = -1;
    if (cg_create(parent) || cg_write(parent, "cgroup.subtree_control", "+memory"))
        goto cleanup;
    /* Same helper operations, with stat before deletion to retain identity.
     * NULL is intentional: that is what the frozen helper passes to fn. */
    for (int i = 0; i < 30; i++) {
        struct stat info;
        char *child = cg_name_indexed(parent, "child", i);
        if (!child)
            goto cleanup;
        int failed = cg_create(child) || stat(child, &info) ||
                     cg_run(child, alloc_dcache, NULL);
        if (!failed)
            children[i].inode = info.st_ino;
        failed |= cg_destroy(child);
        free(child);
        if (failed)
            goto cleanup;
    }
    for (int second = 0; second <= 20; second++) {
        long dead = cg_read_key_long(parent, "cgroup.stat", "nr_dying_descendants ");
        if (dead < 0)
            goto cleanup;
        printf("second=%d dying=%ld current=%ld\n", second, dead,
               cg_read_long(parent, "memory.current"));
        if (second != 20)
            sleep(1);
    }
    char statistics[16384];
    if (cg_read(parent, "memory.stat", statistics, sizeof(statistics)))
        goto cleanup;
    printf("parent-memory-stat\n%s", statistics);
    owners = fopen("/proc/kpagecgroup", "rb");
    flags = open("/proc/kpageflags", O_RDONLY);
    mappings = open("/proc/kpagecount", O_RDONLY);
    if (!owners || flags < 0 || mappings < 0)
        goto cleanup;
    uint64_t chunk[512];
    unsigned long long pfn = 0;
    size_t count;
    while ((count = fread(chunk, sizeof(chunk[0]), 512, owners))) {
        for (size_t n = 0; n < count; n++, pfn++) {
            if (!chunk[n])
                continue;
            for (int i = 0; i < 30; i++) {
                if (chunk[n] != children[i].inode)
                    continue;
                uint64_t page_flags, page_mappings;
                if (pread(flags, &page_flags, sizeof(page_flags), pfn * 8) != 8 ||
                    pread(mappings, &page_mappings, sizeof(page_mappings), pfn * 8) != 8)
                    goto cleanup;
                if (!children[i].pages)
                    children[i].first_pfn = pfn;
                children[i].pages++;
                children[i].flags |= page_flags;
                children[i].mappings += page_mappings;
            }
        }
    }
    if (ferror(owners))
        goto cleanup;
    for (int i = 0; i < 30; i++)
        printf("child=%d inode=%llu pages=%llu flags=0x%llx mappings=%llu first_pfn=%llu\n",
               i, children[i].inode, children[i].pages, children[i].flags,
               children[i].mappings, children[i].first_pfn);
    printf("scanned_pfns=%llu\n", pfn);
    /* Observation completion, never a replacement verdict for the selftest. */
    result = 0;
cleanup:
    if (owners)
        fclose(owners);
    if (flags >= 0)
        close(flags);
    if (mappings >= 0)
        close(mappings);
    if (cg_destroy(parent))
        result = 1;
    free(parent);
    return result;
}
