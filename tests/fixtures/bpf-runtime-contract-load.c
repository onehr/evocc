/* Load/unload the small signed prerequisite probe; a successful boot alone
 * is not a result for this test. */
#include <fcntl.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
    int fd, loaded, unloaded = -1;
    puts("TAP version 13\n1..1");
    fd = open("/tests/bpf-runtime-contract.ko", O_RDONLY);
    loaded = fd < 0 ? -1 : syscall(SYS_finit_module, fd, "", 0);
    if (fd >= 0) close(fd);
    if (!loaded)
        unloaded = syscall(SYS_delete_module, "bpf_runtime_contract", 0);
    printf("%s 1 PMU and resilient-lock contracts, module unloaded\n",
           !loaded && !unloaded ? "ok" : "not ok");
    return loaded || unloaded ? 1 : 0;
}
