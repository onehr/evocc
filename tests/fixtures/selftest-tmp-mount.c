/* Reproduce openat2's mountpoint prerequisite without changing its test. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/vfs.h>
#include <unistd.h>

#ifndef SELF_BIND
#define SELF_BIND 0
#endif

int main(void)
{
    char directory[] = "/scratch/tmp-mount.XXXXXX", file[128], neighbor[128];
    struct stat before, after;
    struct statfs filesystem;
    if (unshare(CLONE_NEWNS) || mount(NULL, "/", NULL, MS_REC | MS_PRIVATE, NULL)
        || !mkdtemp(directory) || chmod(directory, 01777)) {
        perror("namespace and ext4 directory");
        return 2;
    }
    snprintf(file, sizeof(file), "%s/sentinel", directory);
    snprintf(neighbor, sizeof(neighbor), "%s.plain", directory);
    int fd = open(file, O_RDWR | O_CREAT | O_EXCL, 0600);
    if (fd < 0 || write(fd, "x", 1) != 1 || fstat(fd, &before)
        || statfs(directory, &filesystem) || filesystem.f_type != 0xef53
        || mkdir(neighbor, 0700)) {
        perror("disk-backed sentinel");
        return 2;
    }
    if (SELF_BIND && mount(directory, directory, NULL, MS_BIND, NULL)) {
        perror("self-bind");
        return 2;
    }
    errno = 0;
    int rc = mount("", directory, "", MS_PRIVATE, "");
    int mount_errno = errno;
    char byte = 0;
    int preserved = !stat(file, &after) && before.st_dev == after.st_dev
        && before.st_ino == after.st_ino && pread(fd, &byte, 1, 0) == 1 && byte == 'x'
        && !statfs(directory, &filesystem) && filesystem.f_type == 0xef53
        && !stat(directory, &after) && (after.st_mode & 07777) == 01777;
    errno = 0;
    int plain_rc = mount("", neighbor, "", MS_PRIVATE, "");
    int plain_errno = errno;
    printf("TAP version 13\n1..1\n# self_bind=%d private_rc=%d errno=%d preserved=%d plain_rc=%d plain_errno=%d\n",
           SELF_BIND, rc, mount_errno, preserved, plain_rc, plain_errno);
    int pass = rc == 0 && preserved && plain_rc == -1 && plain_errno == EINVAL;
    printf("%s 1 disk-backed directory supports mount propagation\n", pass ? "ok" : "not ok");
    if (close(fd) || (SELF_BIND && umount(directory)) || unlink(file)
        || rmdir(directory) || rmdir(neighbor)) {
        perror("cleanup");
        return 2;
    }
    return pass ? 0 : 1;
}
