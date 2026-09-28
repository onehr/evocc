/* Two owned loop devices share backing storage, as in stress_04. Synchronize
 * their running-daemon exits only after both have real queued I/O. */
#define _GNU_SOURCE
static int shared_before_kill(int ready);
#define UBLK_WARM_LOOP 1
#define UBLK_RUNNING_KILL 1
#define UBLK_BEFORE_KILL(ready) shared_before_kill(ready)
#define main single_device_control_main
#include "ublk-kill-cancel.c"
#undef main
#include <poll.h>

static int ready_fd, release_fd;

static int shared_before_kill(int ready)
{
    unsigned char byte = (unsigned char)(ready != 0);
    if (write(ready_fd, &byte, 1) != 1) return 1;
    struct pollfd wait = {.fd = release_fd, .events = POLLIN};
    return poll(&wait, 1, 10000) != 1 || read(release_fd, &byte, 1) != 1 || byte != 1;
}

static int pair(const char *first, const char *second)
{
    int ready[2], release[2];
    if (pipe2(ready, O_CLOEXEC) || pipe2(release, O_CLOEXEC)) return 1;
    pid_t children[2];
    const char *modes[] = {first, second};
    for (unsigned i = 0; i < 2; ++i) {
        children[i] = fork();
        if (children[i] < 0) abort();
        if (!children[i]) {
            close(ready[0]); close(release[1]);
            ready_fd = ready[1]; release_fd = release[0];
            int bad = probe(modes[i]);
            close(ready_fd); close(release_fd);
            _exit(bad ? 1 : 0);
        }
    }
    close(ready[1]); close(release[0]);
    unsigned count = 0;
    int bad = 0;
    double end = now() + 12;
    while (count != 2 && now() < end) {
        struct pollfd wait = {.fd = ready[0], .events = POLLIN};
        int state = poll(&wait, 1, 100);
        if (state < 0 && errno == EINTR) continue;
        if (state < 0) { bad = 1; break; }
        if (state == 0) continue;
        unsigned char byte;
        if (read(ready[0], &byte, 1) != 1) { bad = 1; break; }
        bad |= byte != 1;
        count++;
    }
    bad |= count != 2;
    printf("# concurrent ready=%u/2, valid=%d; shared backing cancellation\n", count, !bad);
    unsigned char go[] = {1, 1};
    /* This is a local supervisor pipe; no guest daemon is signaled here. */
    signal(SIGPIPE, SIG_IGN);
    if (write(release[1], go, sizeof go) != sizeof go) bad = 1;
    close(ready[0]); close(release[1]);
    for (unsigned i = 0; i < 2; ++i) {
        int status = -1;
        int timeout = finish(children[i], 32, &status);
        printf("# pair worker=%u timeout=%d wait_status=%d\n", i, timeout, status);
        bad |= timeout || !WIFEXITED(status) || WEXITSTATUS(status) != 0;
    }
    return bad;
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..3");
    if (system("modprobe ublk_drv")) return 2;
    int fd = open("/tmp/k1-ublk-cancel-backing", O_RDWR | O_CREAT | O_EXCL, 0600);
    if (fd < 0 || ftruncate(fd, 64 * 1024 * 1024) || close(fd)) return 2;
    const char *modes[] = {"-z --no_ublk_fixed_fd", "--auto_zc", "--nthreads 8 --per_io_tasks"};
    for (unsigned i = 0; i < 3; ++i) {
        int bad = pair(modes[i], modes[(i + 1) % 3]);
        printf("%s %u - shared-backing live cancellation pair %u\n", bad ? "not ok" : "ok", i + 1, i);
        if (bad) {
            puts("# stop at first failure; later cases NOT RUN");
            return 1;
        }
    }
    return 0;
}
