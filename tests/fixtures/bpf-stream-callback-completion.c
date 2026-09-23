/* K10: diagnose completion versus the first read of a BPF error stream.
 * Uses the retained upstream stream.skel.h, hence unchanged BPF instructions.
 * The polling bound belongs only to this diagnostic, not test_progs acceptance. */
#include <bpf/bpf.h>
#include <bpf/libbpf.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
typedef __u64 u64; /* The retained BPF skeleton uses its source's vmlinux alias. */
#include "stream.skel.h"

static int check(int callback)
{
    struct stream *skel = stream__open();
    if (!skel) return 0;
    struct bpf_program *selected = callback ? skel->progs.stream_arena_callback_fault
                                           : skel->progs.stream_arena_subprog_fault;
    struct bpf_program *prog;
    bpf_object__for_each_program(prog, skel->obj)
        bpf_program__set_autoload(prog, prog == selected);
    int ok = 0;
    if (stream__load(skel)) goto out;
    int fd = bpf_program__fd(selected);
    LIBBPF_OPTS(bpf_test_run_opts, run);
    if (bpf_prog_test_run_opts(fd, &run) || run.retval) goto out;
    char text[16385] = {0};
    LIBBPF_OPTS(bpf_prog_stream_read_opts, read_opts);
    int first = bpf_prog_stream_read(fd, 2, text, sizeof(text) - 1, &read_opts);
    int bytes = first, polls = 0;
    while (bytes == 0 && polls < 100) {
        usleep(10000);
        polls++;
        bytes = bpf_prog_stream_read(fd, 2, text, sizeof(text) - 1, &read_opts);
    }
    printf("# callback=%d first_read=%d final_read=%d polls=%d\n", callback, first, bytes, polls);
    if (bytes > 0) {
        text[bytes] = 0;
        ok = strstr(text, "ERROR: Arena WRITE access at unmapped address") != NULL;
        printf("# fault_record=%d\n", ok);
    }
out:
    stream__destroy(skel);
    return ok;
}

int main(void)
{
    puts("TAP version 13\n1..2");
    int sync = check(0), async = check(1);
    printf("%s 1 synchronous original arena fault reaches stream\n", sync ? "ok" : "not ok");
    printf("%s 2 original timer callback fault reaches stream within diagnostic bound\n",
           async ? "ok" : "not ok");
    return !(sync && async);
}
