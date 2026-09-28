/* Follow-up to the batched AIO control: keep the queue populated as
 * completions arrive. A completion is not required to preserve submission
 * order at depth > 1. Every identity, address, length and uniqueness is checked.
 * The shared fixture provides device setup and the depth-one control. */
#define main batched_control_main
#include "ublk-aio-completion-order.c"
#undef main

static int refill_writes(unsigned device, unsigned depth)
{
    char path[64];
    snprintf(path, sizeof(path), "/dev/ublkb%u", device);
    int fd = open(path, O_WRONLY | O_DIRECT | O_CLOEXEC);
    if (fd < 0) return 1;
    aio_context_t context = 0;
    unsigned char *seen = calloc(REQUESTS, 1);
    void *buffer = NULL;
    struct iocb requests[MAX_DEPTH] = {{0}};
    struct iocb *pending[MAX_DEPTH];
    unsigned queued = 0, submitted = 0, completed = 0, displaced = 0, backward = 0, previous = 0;
    int failed = !seen || posix_memalign(&buffer, BLOCK, BLOCK * depth);
    if (!failed && syscall(SYS_io_setup, depth, &context)) failed = 1;
    if (!failed) {
        memset(buffer, 0x5a, BLOCK * depth);
        for (unsigned i = 0; i < depth; ++i) {
            requests[i].aio_data = queued++;
            requests[i].aio_lio_opcode = IOCB_CMD_PWRITE;
            requests[i].aio_fildes = fd;
            requests[i].aio_buf = (uintptr_t)buffer + i * BLOCK;
            requests[i].aio_nbytes = BLOCK;
            requests[i].aio_offset = (int64_t)i * BLOCK;
            pending[i] = &requests[i];
        }
    }
    unsigned count = depth;
    while (!failed && completed < REQUESTS) {
        unsigned sent = 0;
        while (sent < count) {
            long n = syscall(SYS_io_submit, context, count - sent, pending + sent);
            if (n < 0 && errno == EINTR) continue;
            if (n <= 0) { failed = 1; break; }
            sent += (unsigned)n;
            submitted += (unsigned)n;
        }
        if (failed) break;
        struct io_event events[MAX_DEPTH];
        struct timespec timeout = {5, 0};
        long n = syscall(SYS_io_getevents, context, 1, depth, events, &timeout);
        if (n < 0 && errno == EINTR) { count = 0; continue; }
        if (n <= 0) { failed = 1; break; }
        count = 0;
        for (long i = 0; i < n; ++i) {
            uint64_t id = events[i].data;
            unsigned slot = 0;
            while (slot < depth && events[i].obj != (uintptr_t)&requests[slot]) ++slot;
            if (slot == depth || id >= submitted || seen[id] ||
                requests[slot].aio_data != id || events[i].res != BLOCK || events[i].res2) {
                fprintf(stderr, "invalid refill completion id=%llu slot=%u result=%lld/%lld\n",
                        (unsigned long long)id, slot, (long long)events[i].res,
                        (long long)events[i].res2);
                failed = 1;
                break;
            }
            seen[id] = 1;
            if (id != completed) ++displaced;
            if (completed && id < previous) ++backward;
            if ((id != completed || (completed && id < previous)) && displaced <= 8)
                printf("# completion position=%u id=%llu previous=%u\n",
                       completed, (unsigned long long)id, previous);
            previous = (unsigned)id;
            ++completed;
            if (queued < REQUESTS) {
                requests[slot].aio_data = queued;
                requests[slot].aio_offset = (int64_t)queued++ * BLOCK;
                pending[count++] = &requests[slot];
            }
        }
    }
    if (context && syscall(SYS_io_destroy, context)) failed = 1;
    if (submitted != REQUESTS || completed != REQUESTS || (depth == 1 && displaced)) failed = 1;
    printf("# refill depth=%u submitted=%u completed=%u/%u displaced=%u backward=%u errors=%d\n",
           depth, submitted, completed, REQUESTS, displaced, backward, failed);
    free(buffer);
    free(seen);
    close(fd);
    return failed;
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..4");
    if (system("modprobe ublk_drv")) return 2;
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
                    if (start && sscanf(start, "dev id %u", &device) == 1) ++found;
                }
                status = pclose(pipe);
            }
            int bad = found != 1 || status != 0;
            if (!bad) {
                bad = system("udevadm settle") != 0;
                if (!bad) bad = refill_writes(device, depth);
                snprintf(command, sizeof(command), "/tests/ublk/kublk del -n %u >/dev/null", device);
                bad |= system(command) != 0;
            }
            printf("%s %u refill null queues=%u depth=%u identities and lengths\n",
                   bad ? "not ok" : "ok", ++number, queues, depth);
            failed += !!bad;
        }
    }
    return !!failed;
}
