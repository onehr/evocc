/* Diagnostic only: bound one unmodified test_bpf module initialization and
 * retain its last kernel messages/wait location. A timeout remains failure;
 * this does not waive the original all-mode test_kmod.sh acceptance. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static char messages[1 << 20];

#ifndef TEST_HARDENING
#define TEST_HARDENING "2"
#endif
#ifndef TEST_PARAMETERS
#define TEST_PARAMETERS ""
#endif
#ifndef TEST_BUDGET_SECONDS
#define TEST_BUDGET_SECONDS 20
#endif

static int setting(const char *path, const char *value)
{
    int fd = open(path, O_WRONLY);
    if (fd < 0)
        return -1;
    size_t n = strlen(value);
    int ok = write(fd, value, n) == (ssize_t)n;
    close(fd);
    return ok ? 0 : -1;
}

static void dump_file(const char *path, const char *name)
{
    char line[4096];
    FILE *f = fopen(path, "r");
    printf("# %s_BEGIN errno=%d\n", name, f ? 0 : errno);
    if (f) {
        while (fgets(line, sizeof(line), f))
            printf("# %s | %s", name, line);
        fclose(f);
    }
    printf("\n# %s_END\n", name);
}

#ifdef TEST_PROFILE
static int begin_profile(void)
{
    return mkdir("/tmp/trace", 0700)
        || mount("tracefs", "/tmp/trace", "tracefs", 0, NULL)
        || setting("/tmp/trace/buffer_size_kb", "4")
        || setting("/tmp/trace/kprobe_events",
                   "p:k1patch bpf_patch_insn_single pos=$arg2:u32 width=$arg4:u32\n")
        || setting("/tmp/trace/events/kprobes/k1patch/enable", "1");
}

static void profile(void)
{
    dump_file("/tmp/trace/kprobe_profile", "PATCH_HITS");
    dump_file("/tmp/trace/trace", "PATCH_TRACE");
}
#else
static int begin_profile(void) { return 0; }
static void profile(void) { }
#endif

static void snapshot(pid_t child, const char *name)
{
    char path[128], label[128];
    snprintf(path, sizeof(path), "/proc/%ld/%s", (long)child, name);
    snprintf(label, sizeof(label), "CHILD_%s", name);
    dump_file(path, label);
}

static void kernel_tail(void)
{
    int n = syscall(SYS_syslog, 3, messages, sizeof(messages) - 1);
    printf("# KERNEL_TAIL_BEGIN bytes=%d errno=%d\n", n, n < 0 ? errno : 0);
    if (n > 0) {
        messages[n] = 0;
        int start = n, lines = 0;
        while (start && lines < 100)
            lines += messages[--start] == '\n';
        if (start)
            ++start;
        for (char *p = messages + start, *next; *p; p = next) {
            next = strchr(p, '\n');
            if (next)
                *next++ = 0;
            else
                next = p + strlen(p);
            printf("# kernel | %s\n", p);
        }
    }
    puts("# KERNEL_TAIL_END");
}

int main(int argc, char **argv)
{
    const char *hardening = argc > 1 ? argv[1] : TEST_HARDENING;
    const char *parameters = argc > 2 ? argv[2] : TEST_PARAMETERS;
    if (argc > 3 || strlen(hardening) != 1 || !strchr("012", *hardening))
        return 2;
    setvbuf(stdout, NULL, _IONBF, 0);
    puts("TAP version 13\n1..1");
    printf("# diagnostic hardening=%s parameters=%s budget=%ds\n",
           hardening, parameters, TEST_BUDGET_SECONDS);
    if (setting("/proc/sys/net/core/bpf_jit_enable", "1")
        || setting("/proc/sys/net/core/bpf_jit_harden", hardening)
        || begin_profile()) {
        printf("not ok 1 cannot establish diagnostic controls errno=%d\n", errno);
        return 1;
    }
    int fd = open("/tests/test_bpf.ko", O_RDONLY);
    if (fd < 0) {
        printf("not ok 1 cannot open matching signed module errno=%d\n", errno);
        return 1;
    }
    pid_t child = fork();
    if (child < 0) {
        close(fd);
        puts("not ok 1 cannot create observer child");
        return 1;
    }
    if (!child) {
        int rc = syscall(SYS_finit_module, fd, parameters, 0);
        printf("# MODULE_INIT_END rc=%d errno=%d\n", rc, rc ? errno : 0);
        close(fd);
        _exit(rc ? 1 : 0);
    }
    close(fd);
    struct timespec began, now;
    clock_gettime(CLOCK_MONOTONIC, &began);
    int status = 0, finished = 0, sampled = 0;
    do {
        pid_t got = waitpid(child, &status, WNOHANG);
        if (got == child) {
            finished = 1;
            break;
        }
        if (got < 0)
            break;
        usleep(100000);
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (!sampled && now.tv_sec - began.tv_sec >= 5) {
            puts("# PROGRESS_SAMPLE seconds=5");
            snapshot(child, "stat");
            snapshot(child, "stack");
            profile();
            sampled = 1;
        }
    } while (now.tv_sec - began.tv_sec < TEST_BUDGET_SECONDS);
    if (!finished) {
        snapshot(child, "status");
        snapshot(child, "stat");
        snapshot(child, "wchan");
        snapshot(child, "stack");
    }
    profile();
    kernel_tail();
    if (!finished) {
        kill(child, SIGKILL);
        puts("not ok 1 module initialization did not finish within diagnostic budget");
        return 124;
    }
    int ok = WIFEXITED(status) && WEXITSTATUS(status) == 0;
    if (ok) {
        int rc = syscall(SYS_delete_module, "test_bpf", O_NONBLOCK);
        printf("# MODULE_UNLOAD rc=%d errno=%d\n", rc, rc ? errno : 0);
        ok = rc == 0;
    }
    printf("%s 1 unmodified module initializes and unloads in diagnostic budget\n",
           ok ? "ok" : "not ok");
    return !ok;
}
