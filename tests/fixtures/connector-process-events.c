/* Bounded local process-event controls; the upstream listener is unchanged. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/cn_proc.h>
#include <linux/connector.h>
#include <linux/netlink.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int checks, failures;
static void check(int good, const char *name)
{
    printf("%s %d - %s\n", good ? "ok" : "not ok", ++checks, name);
    failures += !good;
}

static long long now_ms(void)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts)) abort();
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

static int subscribe(int fd, enum proc_cn_mcast_op op)
{
    union { struct nlmsghdr align; unsigned char bytes[128]; } packet = {0};
    struct nlmsghdr *header = (void *)packet.bytes;
    struct cn_msg *msg = NLMSG_DATA(header);
    header->nlmsg_len = NLMSG_LENGTH(sizeof(*msg) + sizeof(op));
    header->nlmsg_type = NLMSG_DONE;
    header->nlmsg_pid = (unsigned)getpid();
    msg->id.idx = CN_IDX_PROC;
    msg->id.val = CN_VAL_PROC;
    msg->seq = 71;
    msg->ack = 93;
    msg->len = sizeof(op);
    memcpy(msg->data, &op, sizeof(op));
    return send(fd, packet.bytes, header->nlmsg_len, 0) == header->nlmsg_len;
}

static int receive(int fd, pid_t child, int *forked, int *exited)
{
    long long end = now_ms() + 5000;
    while (now_ms() < end) {
        struct pollfd p = {.fd = fd, .events = POLLIN};
        if (poll(&p, 1, 100) <= 0) continue;
        union { struct nlmsghdr align; unsigned char bytes[4096]; } packet;
        ssize_t count = recv(fd, packet.bytes, sizeof packet.bytes, 0);
        if (count < 0) return 0;
        int length = (int)count;
        struct nlmsghdr *header = (void *)packet.bytes;
        for (; length >= 0 && NLMSG_OK(header, (unsigned)length);
             header = NLMSG_NEXT(header, length)) {
            if (header->nlmsg_type != NLMSG_DONE ||
                NLMSG_PAYLOAD(header, 0) < sizeof(struct cn_msg)) continue;
            struct cn_msg *msg = NLMSG_DATA(header);
            if (msg->id.idx != CN_IDX_PROC || msg->id.val != CN_VAL_PROC ||
                msg->len < sizeof(struct proc_event) ||
                NLMSG_PAYLOAD(header, 0) < sizeof(*msg) + msg->len) continue;
            struct proc_event ev;
            memcpy(&ev, msg->data, sizeof ev);
            /* cn_proc's send_msg replaces seq with its per-CPU counter;
             * cn_proc_ack retains request ack + 1, not the request seq. */
            if (!child && ev.what == PROC_EVENT_NONE &&
                msg->ack == 94) return ev.event_data.ack.err == 0;
            if (child && ev.what == PROC_EVENT_FORK &&
                ev.event_data.fork.child_pid == child &&
                ev.event_data.fork.parent_pid == getpid()) *forked = 1;
            if (child && ev.what == PROC_EVENT_EXIT &&
                ev.event_data.exit.process_pid == child &&
                ev.event_data.exit.exit_code == (23U << 8)) *exited = 1;
            if (child && *forked && *exited) return 1;
        }
    }
    return 0;
}

static void direct_events(void)
{
    int fd = socket(AF_NETLINK, SOCK_DGRAM | SOCK_CLOEXEC, NETLINK_CONNECTOR);
    struct sockaddr_nl address = {.nl_family = AF_NETLINK,
                                 .nl_groups = CN_IDX_PROC, .nl_pid = (unsigned)getpid()};
    int ready = fd >= 0 && bind(fd, (void *)&address, sizeof address) == 0;
    check(ready, "connector socket and process group bind");
    int forked = 0, exited = 0;
    ready = ready && subscribe(fd, PROC_CN_MCAST_LISTEN) &&
            receive(fd, 0, &forked, &exited);
    check(ready, "process subscription receives successful matching acknowledgement");
    pid_t child = ready ? fork() : -1;
    if (child == 0) _exit(23);
    int status = 0;
    int reaped = child > 0 && waitpid(child, &status, 0) == child &&
                 WIFEXITED(status) && WEXITSTATUS(status) == 23;
    if (reaped) receive(fd, child, &forked, &exited);
    check(reaped && forked, "observed fork identifies the owned child and parent");
    check(reaped && exited, "observed exit identifies the owned child and status 23");
    int stopped = fd >= 0 && subscribe(fd, PROC_CN_MCAST_IGNORE);
    if (fd >= 0) stopped = close(fd) == 0 && stopped;
    check(stopped, "unsubscribe and close the owned connector socket");
}

static void original_listener(void)
{
    int pipefd[2];
    if (pipe2(pipefd, O_CLOEXEC)) abort();
    pid_t listener = fork();
    if (listener < 0) abort();
    if (!listener) {
        if (dup2(pipefd[1], 1) < 0 || dup2(pipefd[1], 2) < 0) _exit(126);
        close(pipefd[0]); close(pipefd[1]);
        execl("/tests/original-proc-filter", "proc_filter", NULL);
        _exit(127);
    }
    close(pipefd[1]);
    /* Exercise only our own bounded children. Repetition avoids pretending
     * that a fixed startup sleep is an acknowledgement from the listener. */
    pid_t children[20];
    int owned = 0;
    for (int i = 0; i < 20; ++i) {
        usleep(100000);
        pid_t child = fork();
        if (child == 0) _exit(23);
        int status;
        if (child > 0 && waitpid(child, &status, 0) == child &&
            WIFEXITED(status) && WEXITSTATUS(status) == 23) children[owned++] = child;
    }
    check(owned == 20, "twenty owned process stimuli complete");
    int status = 0;
    pid_t state = waitpid(listener, &status, WNOHANG);
    check(state == 0, "unchanged original remains a listener before SIGINT");
    int signaled = state == 0 && kill(listener, SIGINT) == 0;
    char text[16384];
    size_t used = 0;
    if (fcntl(pipefd[0], F_SETFL, O_NONBLOCK)) abort();
    long long end = now_ms() + 5000;
    while (state == 0 && now_ms() < end) {
        ssize_t n = read(pipefd[0], text + used, sizeof text - 1 - used);
        if (n > 0) used += (size_t)n;
        state = waitpid(listener, &status, WNOHANG);
        usleep(10000);
    }
    check(signaled && state == listener && WIFEXITED(status) && WEXITSTATUS(status) == 0,
          "owned SIGINT ends original normally within five seconds");
    if (state == 0) { kill(listener, SIGKILL); waitpid(listener, &status, 0); }
    ssize_t n;
    while (used < sizeof text - 1 &&
           (n = read(pipefd[0], text + used, sizeof text - 1 - used)) > 0) used += (size_t)n;
    text[used] = '\0';
    close(pipefd[0]);
    int matched = 0;
    for (int i = 0; i < owned; ++i) {
        char fork_line[128], exit_line[128];
        snprintf(fork_line, sizeof fork_line, "Fork process %d (tgid %d), parent %d (tgid %d)",
                 children[i], children[i], getpid(), getpid());
        snprintf(exit_line, sizeof exit_line, "Exit process %d (tgid %d) with code %d,",
                 children[i], children[i], 23 << 8);
        if (strstr(text, fork_line) && strstr(text, exit_line)) matched++;
    }
    check(matched > 0, "original reports matching fork and exit for our own children");
    check(used < sizeof text - 1 && strstr(text, "Done total count:") != NULL,
          "original reports its normal completion without truncated capture");
    printf("# original listener matched %d/%d owned children; captured %zu bytes\n",
           matched, owned, used);
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..10");
    direct_events();
    original_listener();
    return failures != 0;
}
