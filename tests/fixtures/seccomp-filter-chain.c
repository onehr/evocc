/* The frozen seccomp benchmark's two filter forms, without timing assertions.
 * Correct syscall values and enforced denial are distinct from an assumption
 * that measured per-filter costs are linear or stable to ten percent. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <unistd.h>

static unsigned checks;
static void check(int ok, const char *name)
{
    printf("%s %u - %s\n", ok ? "ok" : "not ok", ++checks, name);
    if (!ok)
        exit(1);
}

static int add_filter(struct sock_filter *instructions, unsigned short count)
{
    struct sock_fprog program = { .len = count, .filter = instructions };
    return prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &program);
}

static void unchanged_pid(long pid, const char *stage)
{
    for (unsigned n = 0; n < 1024; ++n) {
        if (syscall(SYS_getpid, n) != pid) {
            check(0, stage);
            return;
        }
    }
    check(1, stage);
}

int main(void)
{
    struct sock_filter bitmap[] = {
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    struct sock_filter full[] = {
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args[0])),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    struct sock_filter deny[] = {
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_getpid, 0, 1),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | EPERM),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    struct sock_filter invalid[] = {
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS, sizeof(struct seccomp_data)),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    setbuf(stdout, NULL);
    puts("TAP version 13\n1..14");
    long pid = syscall(SYS_getpid);
    long parent = syscall(SYS_getppid);
    unchanged_pid(pid, "unfiltered values");
    check(prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0, "no_new_privs");
    check(add_filter(bitmap, 2) == 0, "install first bitmap form");
    unchanged_pid(pid, "one bitmap values");
    check(add_filter(bitmap, 2) == 0, "install second bitmap form");
    unchanged_pid(pid, "two bitmap values");
    check(add_filter(full, 2) == 0, "install argument-reading form");
    unchanged_pid(pid, "three-filter values");
    check(add_filter(bitmap, 2) == 0, "install fourth filter");
    unchanged_pid(pid, "four-filter values");
    errno = 0;
    check(add_filter(invalid, 2) == -1 && errno == EINVAL, "reject out-of-bounds filter load");
    check(add_filter(deny, 4) == 0, "install getpid denial");
    errno = 0;
    check(syscall(SYS_getpid) == -1 && errno == EPERM, "denial wins over prior allow filters");
    check(syscall(SYS_getppid) == parent, "unrelated syscall remains allowed");
    return checks == 14 ? 0 : 1;
}
