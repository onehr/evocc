/* Diagnose absent connector support without confusing it with netlink failure. */
#include <errno.h>
#include <linux/netlink.h>
#include <stdio.h>
#include <sys/socket.h>
#include <unistd.h>

static int checks, failures;
static void check(int good, const char *name)
{
    printf("%s %d - %s\n", good ? "ok" : "not ok", ++checks, name);
    failures += !good;
}

int main(void)
{
    printf("TAP version 13\n1..6\n");
    int protocols[] = {NETLINK_ROUTE, NETLINK_GENERIC};
    for (unsigned i = 0; i < sizeof protocols / sizeof *protocols; ++i) {
        int fd = socket(AF_NETLINK, SOCK_DGRAM, protocols[i]);
        check(fd >= 0, "ordinary netlink protocol is available");
        check(fd >= 0 && close(fd) == 0, "ordinary socket closes");
    }
    errno = 0;
    int fd = socket(AF_NETLINK, SOCK_DGRAM, NETLINK_CONNECTOR);
    int error = errno;
    check(fd == -1, "connector protocol is absent");
    check(error == EPROTONOSUPPORT, "absence reports the expected protocol error");
    if (fd != -1) close(fd);
    printf("# connector fd=%d errno=%d; checks=%d failures=%d\n",
           fd, error, checks, failures);
    return failures != 0;
}
