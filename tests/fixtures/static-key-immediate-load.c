#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    puts("TAP version 13\n1..1");
    int fd = open("/tests/static-key-immediate.ko", O_RDONLY);
    if (argc > 2)
        return 2;
    if (fd < 0 || syscall(SYS_finit_module, fd, argc == 2 ? argv[1] : "", 0)) {
        printf("not ok 1 - static-key consistency errno=%d\n", errno);
        return 1;
    }
    if (close(fd) || syscall(SYS_delete_module, "static_key_immediate", 0)) {
        printf("not ok 1 - unload errno=%d\n", errno);
        return 1;
    }
    puts("ok 1 - static-key queries agree in stable intervals");
    return 0;
}
