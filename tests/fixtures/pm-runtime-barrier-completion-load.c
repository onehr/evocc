#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
    puts("TAP version 13\n1..1");
    int fd = open("/tests/pm-runtime-barrier-completion.ko", O_RDONLY);
    if (fd < 0 || syscall(SYS_finit_module, fd, "", 0)) {
        printf("not ok 1 - completed resume/barrier errno=%d\n", errno);
        return 1;
    }
    if (close(fd) || syscall(SYS_delete_module, "pm_runtime_barrier_completion", 0)) {
        printf("not ok 1 - unload errno=%d\n", errno);
        return 1;
    }
    puts("ok 1 - completed async resume makes barrier return zero");
    return 0;
}
