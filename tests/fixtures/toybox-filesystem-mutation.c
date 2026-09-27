/* B06: exercise the actual chattr prerequisites on a fresh temporary file.
 * 77 means the environment lacks flags/capability, never a test PASS. */
#include <errno.h>
#include <fcntl.h>
#include <linux/fs.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/vfs.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    char path[1024];
    int length = snprintf(path, sizeof(path), "%s/evocc-flags-XXXXXX", argv[1]);
    if (length < 0 || length >= (int)sizeof(path)) return 2;
    int fd = mkstemp(path);
    if (fd < 0) return 2;
    struct statfs fs;
    long original = 0, flags = 0;
    int result = 1, changed = 0, opened;
    if (fstatfs(fd, &fs)) goto cleanup;
    printf("filesystem=%lx\n", (unsigned long)fs.f_type);
    if (ioctl(fd, FS_IOC_GETFLAGS, &original)) {
        printf("getflags errno=%d\n", errno);
        result = 77;
        goto cleanup;
    }
    flags = original | FS_IMMUTABLE_FL;
    if (ioctl(fd, FS_IOC_SETFLAGS, &flags)) {
        printf("immutable errno=%d\n", errno);
        result = 77;
        goto cleanup;
    }
    changed = 1;
    opened = open(path, O_WRONLY | O_TRUNC);
    if (opened >= 0) { close(opened); goto cleanup; }
    if (errno != EPERM || unlink(path) != -1 || errno != EPERM) goto cleanup;
    if (ioctl(fd, FS_IOC_SETFLAGS, &original)) goto cleanup;
    flags = original | FS_APPEND_FL;
    if (ioctl(fd, FS_IOC_SETFLAGS, &flags)) goto cleanup;
    opened = open(path, O_WRONLY | O_TRUNC);
    if (opened >= 0) { close(opened); goto cleanup; }
    if (errno != EPERM) goto cleanup;
    opened = open(path, O_WRONLY | O_APPEND);
    if (opened < 0) goto cleanup;
    int wrote = write(opened, "x", 1) == 1;
    int closed = close(opened) == 0;
    if (!wrote || !closed) goto cleanup;
    result = 0;
cleanup:
    if (changed && ioctl(fd, FS_IOC_SETFLAGS, &original)) result = 1;
    if (close(fd) || unlink(path)) result = 1;
    if (!result) puts("immutable and append-only enforced; cleanup complete");
    return result;
}
