/* Diagnose FS_IOC_GETFLAGS without changing any file's immutable attributes.
 * Compare the actual mounted benchmark directory with a container-local file.
 * Success means the observations were collected, not that chattr tests pass. */
#include <errno.h>
#include <fcntl.h>
#include <linux/fs.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/vfs.h>
#include <unistd.h>

static int observe(const char *directory)
{
    char path[1024];
    struct statfs filesystem;
    int length = snprintf(path, sizeof(path), "%s/evocc-fsflags-XXXXXX", directory);
    if (length < 0 || length >= (int)sizeof(path)) return 1;
    int fd = mkstemp(path);
    if (fd < 0) return 2;
    if (fstatfs(fd, &filesystem)) {
        close(fd);
        unlink(path);
        return 3;
    }
    long flags = 0;
    errno = 0;
    int result = ioctl(fd, FS_IOC_GETFLAGS, &flags);
    int error = errno;
    printf("%s filesystem=%lx getflags=%d errno=%d flags=%lx\n",
           directory, (unsigned long)filesystem.f_type, result, error, (unsigned long)flags);
    int closed = close(fd);
    int removed = unlink(path);
    return closed != 0 || removed != 0;
}

int main(int argc, char **argv)
{
    if (argc != 3) return 4;
    int first = observe(argv[1]);
    int second = observe(argv[2]);
    return first ? first : second;
}
