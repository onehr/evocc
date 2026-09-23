/* Native loader for the K08 kernel-API reduction. No result is synthesized:
 * module_init returns zero only after checking the real perf event APIs. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
    puts("TAP version 13\n1..1");
    int fd = open("/tests/hw-breakpoint-kernel-task.ko", O_RDONLY);
    if (fd < 0 || syscall(SYS_finit_module, fd, "", 0)) {
        printf("not ok 1 - load/API contract errno=%d\n", errno);
        return 1;
    }
    if (close(fd) || syscall(SYS_delete_module, "hw_breakpoint_kernel_task", 0)) {
        printf("not ok 1 - unload errno=%d\n", errno);
        return 1;
    }
    puts("ok 1 - CPU-wide EL1 admitted; per-task EL1 rejected with EINVAL");
    return 0;
}
