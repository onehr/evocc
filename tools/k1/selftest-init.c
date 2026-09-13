/* PID 1 for upstream kernel selftests. Built by the reference compiler. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/reboot.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int full_suite;

static void poweroff(void)
{
    sync();
    reboot(RB_POWER_OFF);
    for (;;)
        pause();
}

static void fatal(const char *what)
{
    dprintf(1, "KSELFTEST_INIT_ERROR %s: %s\n", what, strerror(errno));
    poweroff();
}

static double monotonic(void)
{
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now))
        fatal("clock_gettime");
    return now.tv_sec + now.tv_nsec / 1e9;
}

static int run(const char *id, unsigned timeout)
{
    char path[256], cwd[256];
    snprintf(path, sizeof(path), "/tests/%s", id);
    snprintf(cwd, sizeof(cwd), "/scratch/%s", id);
    if (!full_suite && mkdir(cwd, 0700) && errno != EEXIST)
        fatal("mkdir test directory");
    pid_t child = fork();
    if (child < 0)
        return 127;
    if (!child) {
        if (setpgid(0, 0) || chdir(full_suite ? "/tests" : cwd))
            _exit(127);
        if (full_suite)
            execl("/tests/run_kselftest.sh", "run_kselftest.sh", "-c", id, (char *)NULL);
        execl(path, path, (char *)NULL);
        _exit(127);
    }
    /* Close the fork/exec race before the parent can time out the group. */
    if (setpgid(child, child) && errno != EACCES && errno != ESRCH)
        fatal("setpgid");
    double deadline = monotonic() + timeout;
    int status;
    for (;;) {
        pid_t done = waitpid(child, &status, WNOHANG);
        if (done == child)
            break;
        if (done < 0 && errno != EINTR)
            fatal("waitpid");
        if (monotonic() >= deadline) {
            kill(-child, SIGKILL);
            while (waitpid(child, &status, 0) < 0 && errno == EINTR) {}
            return 124;
        }
        usleep(10000);
    }
    kill(-child, SIGKILL); /* No lingering worker may affect the next test. */
    while (waitpid(-1, NULL, WNOHANG) > 0) {}
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}

int main(void)
{
    char command[4096], nonce[129], id[200];
    unsigned limit;
    setvbuf(stdout, NULL, _IONBF, 0);
    full_suite = access("/full-suite", F_OK) == 0;
    if (full_suite) {
        if (mount("devtmpfs", "/dev", "devtmpfs", 0, NULL)
            || mount("/dev/vda", "/newroot", "ext4", 0, NULL)
            || chdir("/newroot") || chroot("."))
            fatal("mount selftest runtime root");
    }
    setenv("PATH", "/usr/sbin:/usr/bin:/sbin:/bin", 1);
    setenv("HOME", "/root", 1);
    puts("EVO_INIT_ALIVE");
    if (mount("proc", "/proc", "proc", 0, NULL))
        fatal("mount proc");
    FILE *file = fopen("/proc/cmdline", "r");
    if (!file || !fgets(command, sizeof(command), file))
        fatal("read cmdline");
    fclose(file);
    char *key = strstr(command, "evo_nonce=");
    if (!key || sscanf(key + strlen("evo_nonce="), "%128s", nonce) != 1)
        fatal("missing nonce");
    dprintf(1, "EVO_BOOT_NONCE %s\n", nonce);
    char collection[200] = "";
    key = strstr(command, "evo_selftest=");
    if (key && sscanf(key + strlen("evo_selftest="), "%199s", collection) != 1)
        fatal("invalid collection");
    if (mount("sysfs", "/sys", "sysfs", 0, NULL)
        || mount("devtmpfs", "/dev", "devtmpfs", 0, NULL)
        || mount("tmpfs", "/tmp", "tmpfs", 0, "mode=1777"))
        fatal("mount pseudo-filesystems");
    if (mkdir("/dev/shm", 01777) && errno != EEXIST)
        fatal("mkdir shm");
    if (mount("tmpfs", "/dev/shm", "tmpfs", 0, "mode=1777"))
        fatal("mount shm");
    if (full_suite) {
        if (system("ip link set lo up"))
            fatal("enable loopback");
        const char *types[] = {"debugfs", "tracefs", "cgroup2", "bpf"};
        const char *paths[] = {"/sys/kernel/debug", "/sys/kernel/tracing", "/sys/fs/cgroup", "/sys/fs/bpf"};
        for (unsigned i = 0; i < sizeof(types) / sizeof(types[0]); ++i)
            if (mount(types[i], paths[i], types[i], 0, NULL))
                dprintf(1, "KSELFTEST_SETUP unavailable %s: %s\n", types[i], strerror(errno));
    }
    if (symlink("/proc/self/fd", "/dev/fd") && errno != EEXIST)
        fatal("link fd");
    if (!full_suite && mount("/dev/vda", "/scratch", "ext4", 0, NULL))
        fatal("mount scratch");
    file = fopen("/tests/huge-pages", "r");
    unsigned pages, available;
    if (!file || fscanf(file, "%u", &pages) != 1)
        fatal("read huge-page setup");
    fclose(file);
    file = fopen("/proc/sys/vm/nr_hugepages", "w");
    if (!file || fprintf(file, "%u\n", pages) < 0 || fclose(file))
        fatal("reserve huge pages");
    file = fopen("/proc/sys/vm/nr_hugepages", "r");
    if (!file || fscanf(file, "%u", &available) != 1 || available < pages)
        fatal("verify huge-page reservation");
    fclose(file);
    dprintf(1, "KSELFTEST_SETUP hugepages %u\n", available);
    file = fopen("/tests/list", "r");
    if (!file)
        fatal("read test list");
    unsigned count = 0;
    while (fscanf(file, "%199s %u", id, &limit) == 2) {
        if ((!full_suite && strchr(id, '/')) || strstr(id, "..") || !limit)
            fatal("invalid test entry");
        if (collection[0] && strcmp(collection, id))
            continue;
        dprintf(1, "KSELFTEST_BEGIN %s %s\n", nonce, id);
        int rc = run(id, limit);
        dprintf(1, "KSELFTEST_END %s %s %d\n", nonce, id, rc);
        count++;
    }
    if (!feof(file) || !count)
        fatal("incomplete test list");
    fclose(file);
    puts("EVO_INIT_DONE");
    poweroff();
}
