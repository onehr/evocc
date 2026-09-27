/* Explicit supplement: UUCP unavailable, tmpfs timestamps exact, other
   filesystems rejected rather than assigned a guessed timestamp resolution. */
#define _GNU_SOURCE 1
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/statfs.h>
#include <unistd.h>

static int checks, failures;
#define CHECK(x) do { ++checks; if (!(x)) { ++failures; \
    printf("line=%d errno=%d\n", __LINE__, errno); } } while (0)

int main(void)
{
    char file[] = "/dev/shm/evo-conf-XXXXXX";
    int fd = mkstemp(file);
    if (fd < 0) return 2;
    struct statfs fs;
    if (fstatfs(fd, &fs) || (unsigned long)fs.f_type != 0x01021994) return 3;
    for (int preset = 0; preset <= 1; ++preset) {
        int saved = preset ? EDOM : 0;
        errno = saved;
        CHECK(sysconf(_SC_XOPEN_UUCP) == -1 && errno == saved);
        errno = saved;
        CHECK(fpathconf(fd, _PC_TIMESTAMP_RESOLUTION) == 1 && errno == saved);
        errno = saved;
        CHECK(pathconf(file, _PC_TIMESTAMP_RESOLUTION) == 1 && errno == saved);
        errno = saved;
        CHECK(pathconf("/dev/shm", _PC_TIMESTAMP_RESOLUTION) == 1 && errno == saved);
    }
    struct timespec stamp[2] = {{1234567890, 123456789}, {1234567891, 987654321}};
    struct stat st;
    CHECK(futimens(fd, stamp) == 0);
    CHECK(fstat(fd, &st) == 0 && st.st_atim.tv_nsec == 123456789 &&
          st.st_mtim.tv_nsec == 987654321);
    char link[128];
    snprintf(link, sizeof link, "%s-link", file);
    CHECK(symlink(file, link) == 0);
    CHECK(pathconf(link, _PC_TIMESTAMP_RESOLUTION) == 1);
    CHECK(unlink(link) == 0);
    CHECK(unlink(file) == 0);
    CHECK(fpathconf(fd, _PC_TIMESTAMP_RESOLUTION) == 1);
    CHECK(close(fd) == 0);
    errno = 0;
    CHECK(fpathconf(fd, _PC_TIMESTAMP_RESOLUTION) == -1 && errno == EBADF);
    errno = 0;
    CHECK(pathconf(file, _PC_TIMESTAMP_RESOLUTION) == -1 && errno == ENOENT);
    errno = 0;
    CHECK(pathconf("", _PC_TIMESTAMP_RESOLUTION) == -1 && errno == ENOENT);
    errno = 0;
    CHECK(sysconf(INT_MAX) == -1 && errno == EINVAL);
    errno = 0;
    CHECK(fpathconf(-1, INT_MAX) == -1 && errno == EINVAL);
    errno = 0;
    CHECK(pathconf("/dev/shm", INT_MAX) == -1 && errno == EINVAL);
    /* Procfs is intentionally not assigned an inferred shmem resolution. */
    int proc = open("/proc", O_PATH|O_CLOEXEC);
    if (proc < 0 || fstatfs(proc, &fs) || (unsigned long)fs.f_type == 0x01021994) return 4;
    errno = 0;
    CHECK(fpathconf(proc, _PC_TIMESTAMP_RESOLUTION) == -1 && errno == EINVAL);
    errno = 0;
    CHECK(pathconf("/proc", _PC_TIMESTAMP_RESOLUTION) == -1 && errno == EINVAL);
    CHECK(close(proc) == 0);
    /* Existing names retain their old meaning. */
    CHECK(sysconf(_SC_PAGESIZE) > 0);
    CHECK(pathconf("/dev/shm", _PC_NAME_MAX) == NAME_MAX);
    printf("checks=%d failures=%d\n", checks, failures);
    return failures != 0;
}
