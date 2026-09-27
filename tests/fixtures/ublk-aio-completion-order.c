/* Diagnostic for ublk generic_01/02, not a replacement upstream assertion.
 * Submit ascending 4 KiB writes to the same null target. Check every completion
 * identity and length exactly once; report (do not require) completion order.
 * The null target cannot establish data persistence. Reference-built userspace
 * isolates kernel/runtime behavior from EvoCC userspace code generation. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/aio_abi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

enum { BLOCK = 4096, REQUESTS = 16384, MAX_DEPTH = 16 };

static int completed_writes(unsigned device, unsigned depth)
{
    char path[64];
    snprintf(path, sizeof(path), "/dev/ublkb%u", device);
    int fd = open(path, O_WRONLY | O_DIRECT | O_CLOEXEC);
    if (fd < 0) {
        perror("open ublk device");
        return 1;
    }
    aio_context_t context = 0;
    unsigned char *seen = calloc(REQUESTS, 1);
    void *buffer = NULL;
    int failed = !seen || posix_memalign(&buffer, BLOCK, BLOCK * depth);
    if (!failed && syscall(SYS_io_setup, depth, &context)) {
        perror("io_setup");
        failed = 1;
    }
    unsigned completed = 0, reordered = 0, backward = 0, previous = 0;
    if (!failed)
        memset(buffer, 0xa5, BLOCK * depth);
    for (unsigned first = 0; !failed && first < REQUESTS; first += depth) {
        struct iocb requests[MAX_DEPTH] = {{0}};
        struct iocb *pointers[MAX_DEPTH];
        for (unsigned i = 0; i < depth; ++i) {
            requests[i].aio_data = first + i;
            requests[i].aio_lio_opcode = IOCB_CMD_PWRITE;
            requests[i].aio_fildes = fd;
            requests[i].aio_buf = (uintptr_t)buffer + i * BLOCK;
            requests[i].aio_nbytes = BLOCK;
            requests[i].aio_offset = (int64_t)(first + i) * BLOCK;
            pointers[i] = &requests[i];
        }
        unsigned submitted = 0;
        while (submitted < depth) {
            long n = syscall(SYS_io_submit, context, depth - submitted, pointers + submitted);
            if (n < 0 && errno == EINTR)
                continue;
            if (n <= 0) {
                perror("io_submit");
                failed = 1;
                break;
            }
            submitted += (unsigned)n;
        }
        unsigned received = 0;
        while (received < submitted) {
            struct io_event events[MAX_DEPTH];
            struct timespec timeout = {5, 0};
            long n = syscall(SYS_io_getevents, context, 1, submitted - received, events, &timeout);
            if (n < 0 && errno == EINTR)
                continue;
            if (n <= 0) {
                fprintf(stderr, "io_getevents returned %ld errno=%d\n", n, errno);
                failed = 1;
                break;
            }
            for (long i = 0; i < n; ++i) {
                uint64_t id = events[i].data;
                if (id < first || id >= first + submitted || seen[id]
                    || events[i].res != BLOCK || events[i].res2 != 0
                    || events[i].obj != (uintptr_t)&requests[id - first]) {
                    fprintf(stderr, "invalid completion id=%llu result=%lld/%lld\n",
                            (unsigned long long)id, (long long)events[i].res,
                            (long long)events[i].res2);
                    failed = 1;
                    continue;
                }
                seen[id] = 1;
                if (id != completed)
                    ++reordered;
                if (completed && id < previous)
                    ++backward;
                previous = (unsigned)id;
                ++completed;
            }
            received += (unsigned)n;
        }
    }
    if (context && syscall(SYS_io_destroy, context))
        failed = 1;
    if (completed != REQUESTS || (depth == 1 && reordered))
        failed = 1;
    printf("# depth=%u completed=%u/%u displaced=%u backward=%u errors=%d\n",
           depth, completed, REQUESTS, reordered, backward, failed);
    free(buffer);
    free(seen);
    close(fd);
    return failed;
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..4");
    if (system("modprobe ublk_drv"))
        return 2;
    unsigned number = 0, failed = 0;
    for (unsigned queues = 1; queues <= 2; ++queues) {
        for (unsigned depth = 1; depth <= MAX_DEPTH; depth *= MAX_DEPTH) {
            char command[160];
            snprintf(command, sizeof(command), "/tests/ublk/kublk add -t null -q %u", queues);
            FILE *pipe = popen(command, "r");
            unsigned device = 0;
            int found = 0, status = -1;
            if (pipe) {
                char line[512];
                while (fgets(line, sizeof(line), pipe)) {
                    char *start = strstr(line, "dev id ");
                    if (start && sscanf(start, "dev id %u", &device) == 1)
                        ++found;
                }
                status = pclose(pipe);
            }
            int bad = found != 1 || status != 0;
            if (!bad) {
                bad = system("udevadm settle") != 0;
                if (!bad)
                    bad = completed_writes(device, depth);
                snprintf(command, sizeof(command), "/tests/ublk/kublk del -n %u >/dev/null", device);
                bad |= system(command) != 0;
            }
            printf("%s %u null queues=%u depth=%u exact completion identities and lengths\n",
                   bad ? "not ok" : "ok", ++number, queues, depth);
            failed += !!bad;
        }
    }
    return !!failed;
}
