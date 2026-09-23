/* The udmabuf selftest needs 1024 naturally sized (2 MiB) huge pages.
 * Keep the original 2 GiB request; measure pool reservation, not a reduced
 * workload. This program allocates RAM only and closes every descriptor. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

int main(void)
{
    const size_t huge = 2UL * 1024 * 1024, size = 1024 * huge;
    const long page = sysconf(_SC_PAGESIZE);
    if (page != 4096) return 2;
    int fd = memfd_create("udmabuf-hugetlb-reduction", MFD_HUGETLB | MFD_ALLOW_SEALING);
    if (fd < 0 || fcntl(fd, F_ADD_SEALS, F_SEAL_SHRINK) || ftruncate(fd, size)) {
        perror("hugetlb memfd");
        return 2;
    }
    errno = 0;
    unsigned char *bytes = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    int mapping_errno = errno;
    printf("TAP version 13\n1..1\n# bytes=%zu huge_bytes=%zu mmap_errno=%d\n",
           size, huge, mapping_errno);
    if (bytes == MAP_FAILED) {
        printf("not ok 1 original-size hugetlb reservation: %s\n", strerror(mapping_errno));
        close(fd);
        return 1;
    }
    for (size_t offset = 0; offset < size; offset += huge)
        bytes[offset] = (unsigned char)(offset / huge);
    for (size_t offset = 0; offset < size; offset += huge) {
        if (bytes[offset] != (unsigned char)(offset / huge)) return 3;
    }
    if (munmap(bytes, size) || close(fd)) return 2;
    puts("ok 1 original-size hugetlb reservation and all 1024 page accesses");
    return 0;
}
