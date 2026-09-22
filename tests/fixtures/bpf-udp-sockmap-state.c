/* Diagnostic of the frozen test_maps UDP update, not a replacement test. */
#include <errno.h>
#include <linux/bpf.h>
#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(void)
{
    union bpf_attr create = {
        .map_type = BPF_MAP_TYPE_SOCKMAP,
        .key_size = 4, .value_size = 4, .max_entries = 6,
    };
    int map = syscall(SYS_bpf, BPF_MAP_CREATE, &create, sizeof(create));
    if (map < 0) {
        perror("BPF_MAP_CREATE");
        return 1;
    }
    int udp = socket(AF_INET, SOCK_DGRAM, 0);
    if (udp < 0)
        return 2;
    uint32_t key = 0;
    union bpf_attr update = {
        .map_fd = map,
        .key = (uintptr_t)&key,
        .value = (uintptr_t)&udp,
        .flags = BPF_ANY,
    };
    errno = 0;
    int unbound = syscall(SYS_bpf, BPF_MAP_UPDATE_ELEM, &update, sizeof(update));
    int unbound_errno = errno;
    struct sockaddr_in address = {.sin_family = AF_INET};
    if (bind(udp, (struct sockaddr *)&address, sizeof(address)))
        return 3;
    errno = 0;
    int bound = syscall(SYS_bpf, BPF_MAP_UPDATE_ELEM, &update, sizeof(update));
    int bound_errno = errno;
    printf("unbound_update=%d errno=%d bound_update=%d errno=%d\n",
           unbound, unbound_errno, bound, bound_errno);
    close(udp);
    close(map);
    return !(unbound == -1 && unbound_errno == EOPNOTSUPP && bound == 0);
}
