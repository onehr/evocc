/* K10: the frozen kernel admits UDP socket-map insertion only after bind.
 * Original sockmap tests' opened-but-unbound expectation is not that API.
 * Reference-built diagnostic; no BPF program or libbpf is needed. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/bpf.h>
#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <unistd.h>

static int check(int type, int family)
{
    union bpf_attr attr = {
        .map_type = type, .key_size = 4, .value_size = 8, .max_entries = 1,
    };
    unsigned key = 0;
    uint64_t value;
    int map = syscall(SYS_bpf, BPF_MAP_CREATE, &attr, sizeof(attr));
    int sock, opened, opened_errno, bound = -1, updated = -1;
    if (map < 0) return 0;
    sock = socket(family, SOCK_DGRAM, 0);
    if (sock < 0) { close(map); return 0; }
    value = sock;
    attr = (union bpf_attr){ .map_fd = map, .key = (uintptr_t)&key,
                            .value = (uintptr_t)&value, .flags = BPF_NOEXIST };
    errno = 0;
    opened = syscall(SYS_bpf, BPF_MAP_UPDATE_ELEM, &attr, sizeof(attr));
    opened_errno = errno;
    if (family == AF_INET) {
        struct sockaddr_in address = { .sin_family = AF_INET };
        bound = bind(sock, (struct sockaddr *)&address, sizeof(address));
    } else {
        struct sockaddr_in6 address = { .sin6_family = AF_INET6 };
        bound = bind(sock, (struct sockaddr *)&address, sizeof(address));
    }
    if (!bound)
        updated = syscall(SYS_bpf, BPF_MAP_UPDATE_ELEM, &attr, sizeof(attr));
    printf("# type=%d family=%d opened=%d errno=%d bind=%d bound_update=%d\n",
           type, family, opened, opened_errno, bound, updated);
    close(sock);
    close(map);
    return opened == -1 && opened_errno == EOPNOTSUPP && !bound && !updated;
}

int main(void)
{
    const int types[] = {BPF_MAP_TYPE_SOCKMAP, BPF_MAP_TYPE_SOCKHASH};
    const int families[] = {AF_INET, AF_INET6};
    int i, j, number = 0, failed = 0;
    puts("TAP version 13\n1..4");
    for (i = 0; i < 2; ++i) for (j = 0; j < 2; ++j) {
        int ok = check(types[i], families[j]);
        printf("%s %d UDP insertion requires a bound socket\n", ok ? "ok" : "not ok", ++number);
        failed += !ok;
    }
    return failed != 0;
}
