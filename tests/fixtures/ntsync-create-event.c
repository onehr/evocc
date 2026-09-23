/* K09: run after loading ntsync. Success returns an object fd, not zero.
 * This is a uAPI reduction, not a replacement for the frozen wake_all test. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/ntsync.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <unistd.h>

#define CHECK(expr) do { \
    if (!(expr)) { \
        printf("not ok 1 - line %d: %s (errno=%d)\n", __LINE__, #expr, errno); \
        return 1; \
    } \
} while (0)

int main(void)
{
    struct ntsync_event_args input = { .manual = 0, .signaled = 1 };
    struct ntsync_event_args state = {0};
    unsigned int previous = 0;
    int device, event;

    puts("TAP version 13\n1..1");
    /* Make zero unavailable without assuming the launcher's fd layout. */
    if (fcntl(0, F_GETFD) == -1) {
        int reserved = open("/dev/null", O_RDONLY);
        CHECK(reserved == 0);
    }
    device = open("/dev/ntsync", O_CLOEXEC | O_RDONLY);
    CHECK(device >= 0);
    event = ioctl(device, NTSYNC_IOC_CREATE_EVENT, &input);
    printf("# CREATE_EVENT returned %d; frozen wake_all expects 0\n", event);
    CHECK(event > 0 && event != device);
    CHECK(fcntl(event, F_GETFD) == FD_CLOEXEC);
    CHECK(ioctl(event, NTSYNC_IOC_EVENT_READ, &state) == 0);
    CHECK(state.manual == 0 && state.signaled == 1);
    CHECK(ioctl(event, NTSYNC_IOC_EVENT_RESET, &previous) == 0);
    CHECK(previous == 1);
    CHECK(ioctl(event, NTSYNC_IOC_EVENT_READ, &state) == 0);
    CHECK(state.manual == 0 && state.signaled == 0);
    errno = 0;
    CHECK(ioctl(device, NTSYNC_IOC_CREATE_EVENT, (void *)0) == -1);
    CHECK(errno == EFAULT);
    CHECK(close(event) == 0);
    errno = 0;
    CHECK(ioctl(event, NTSYNC_IOC_EVENT_READ, &state) == -1);
    CHECK(errno == EBADF);
    CHECK(close(device) == 0);
    puts("ok 1 - CREATE_EVENT fd, object state, close and error controls");
    return 0;
}
