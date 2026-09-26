/* K01 observer variant: reuse the exact publication protocol, with predictable
 * mmap addresses so a read-only QEMU plugin can identify the translated code.
 * Not a replacement for the original test or a production workaround. */
#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>

static void *probe_mmap(void *address, size_t size, int prot, int flags,
                        int fd, off_t offset)
{
    static unsigned mapping;
    (void)address;
    void *fixed = (void *)(uintptr_t)(0x3000000000UL + 0x100000UL * mapping++);
    return mmap(fixed, size, prot, flags | MAP_FIXED_NOREPLACE, fd, offset);
}

#define mmap probe_mmap
#define main publication_main
#include "instruction-publish-alias.c"
#undef main
#undef mmap

int main(int argc, char **argv)
{
    printf("# publication flag address=%p; code addresses=3000000000/3000100000\n",
           (void *)&published);
    fflush(stdout);
    return publication_main(argc, argv);
}
