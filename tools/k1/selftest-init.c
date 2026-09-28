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
#include <sys/utsname.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int full_suite;

static _Noreturn void poweroff(void)
{
    sync();
    reboot(RB_POWER_OFF);
    for (;;)
        pause();
}

static _Noreturn void fatal(const char *what)
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

static unsigned huge_page_count(unsigned fallback)
{
    const char *text = getenv("kselftest_hugepages");
    if (!text)
        return fallback;
    size_t length = strlen(text);
    if (!length || length > 4 || strspn(text, "0123456789") != length) {
        errno = EINVAL;
        fatal("invalid per-program huge-page count");
    }
    unsigned long pages = strtoul(text, NULL, 10);
    if (!pages || pages > 1024) {
        errno = EINVAL;
        fatal("per-program huge-page count outside 1..1024");
    }
    return (unsigned)pages;
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
        if (full_suite) {
            const char *override = getenv("kselftest_override_timeout");
            if (override)
                execl("/tests/run_kselftest.sh", "run_kselftest.sh", "-o", override,
                      "-t", id, (char *)NULL);
            execl("/tests/run_kselftest.sh", "run_kselftest.sh", strchr(id, ':') ? "-t" : "-c", id, (char *)NULL);
        }
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

static unsigned load_runtime_modules(const char *path, const char *nonce)
{
    FILE *file = fopen(path, "r");
    if (!file) {
        if (errno == ENOENT && !nonce)
            return 0; /* The original module-free runtime is unchanged. */
        fatal("read module setup");
    }
    unsigned count = 0;
    char name[128];
    while (fgets(name, sizeof(name), file)) {
        size_t length = strcspn(name, "\n");
        if (!length || name[length] != '\n'
            || strspn(name, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-") != length) {
            errno = EINVAL;
            fatal("invalid module setup");
        }
        name[length] = '\0';
        if (nonce) {
            if (system("dmesg --clear"))
                fatal("clear module log boundary");
            dprintf(1, "KUNIT_MODULE_BEGIN %s %s\n", nonce, name);
        }
        pid_t child = fork();
        if (child < 0)
            fatal("fork modprobe");
        if (!child) {
            execlp("modprobe", "modprobe", "--", name, NULL);
            _exit(127);
        }
        int status;
        while (waitpid(child, &status, 0) < 0)
            if (errno != EINTR)
                fatal("wait modprobe");
        int rc = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
        if (nonce) {
            /* Read complete printk records, not the asynchronous console
             * stream: WARN paths can replay a half-written KTAP line after
             * modprobe exits. Keep every record, including failures. */
            if (system("dmesg --read-clear --color=never"))
                fatal("read module KUnit records");
            dprintf(1, "KUNIT_MODULE_END %s %s %d\n", nonce, name, rc);
        } else if (rc) {
            errno = EIO;
            fatal("load required runtime module");
        } else {
            dprintf(1, "KSELFTEST_SETUP module %s loaded\n", name);
        }
        count++;
    }
    if (!feof(file))
        fatal("incomplete module setup");
    fclose(file);
    return count;
}

/* Some built-in suites need algorithms supplied by modules. Re-run only the
 * explicit list after setup, through KUnit's own interface. The earlier boot
 * results remain separate; a successful trigger is not a passing test. */
static unsigned run_kunit_suites(const char *path, const char *nonce, const char *root)
{
    FILE *list = fopen(path, "r");
    if (!list) {
        if (errno == ENOENT)
            return 0;
        fatal("read KUnit rerun list");
    }
    char name[256], file_path[1024], buffer[4096];
    unsigned count = 0;
    while (fgets(name, sizeof(name), list)) {
        size_t length = strcspn(name, "\n");
        if (!length || name[length] != '\n'
            || strspn(name, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-") != length) {
            errno = EINVAL;
            fatal("invalid KUnit suite name");
        }
        name[length] = '\0';
        if (!strcmp(name, ".") || !strcmp(name, "..")) {
            errno = EINVAL;
            fatal("invalid KUnit suite name");
        }
        if (snprintf(file_path, sizeof(file_path), "%s/%s/run", root, name) >= (int)sizeof(file_path)) {
            errno = ENAMETOOLONG;
            fatal("KUnit run path");
        }
        /* No O_CREAT: an init-only suite has no rerun entry. */
        int fd = open(file_path, O_WRONLY);
        if (fd < 0)
            fatal("open KUnit run");
        dprintf(1, "KUNIT_RERUN_BEGIN %s %s\n", nonce, name);
        if (write(fd, "1\n", 2) != 2 || close(fd))
            fatal("trigger KUnit suite");
        if (snprintf(file_path, sizeof(file_path), "%s/%s/results", root, name) >= (int)sizeof(file_path)) {
            errno = ENAMETOOLONG;
            fatal("KUnit results path");
        }
        FILE *results = fopen(file_path, "r");
        if (!results)
            fatal("open KUnit results");
        size_t bytes, total = 0;
        while ((bytes = fread(buffer, 1, sizeof(buffer), results))) {
            total += bytes;
            if (fwrite(buffer, 1, bytes, stdout) != bytes)
                fatal("write KUnit results");
        }
        if (ferror(results) || fclose(results) || fflush(stdout))
            fatal("read KUnit results");
        if (!total) {
            errno = EIO;
            fatal("empty KUnit results");
        }
        dprintf(1, "KUNIT_RERUN_END %s %s\n", nonce, name);
        count++;
    }
    if (ferror(list) || fclose(list))
        fatal("read KUnit rerun list");
    return count;
}

int main(void)
{
    char command[4096], nonce[129], id[200];
    unsigned limit;
    setvbuf(stdout, NULL, _IONBF, 0);
    full_suite = access("/full-suite", F_OK) == 0;
    if (full_suite) {
        /* A chroot alone leaves us below the mount-namespace root, where
         * Linux correctly refuses to create user namespaces. The temporary
         * device mount must not remain as a hidden sibling of the new root:
         * foreign-namespace listmount would select it as the namespace root.
         * /dev is mounted again inside the runtime after the root switch. */
        if (mount("devtmpfs", "/dev", "devtmpfs", 0, NULL)
            || mount("/dev/vda", "/newroot", "ext4", 0, NULL)
            || umount("/dev")
            || chdir("/newroot") || mount(".", "/", NULL, MS_MOVE, NULL)
            || chroot(".") || chdir("/"))
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
    unsigned timeout_override = 0;
    const char *timeout_text = getenv("kselftest_override_timeout");
    if (timeout_text) {
        char *end;
        errno = 0;
        unsigned long seconds = strtoul(timeout_text, &end, 10);
        if (!full_suite || !strchr(collection, ':') || errno || *end
            || !seconds || seconds > 86400)
            fatal("invalid per-program timeout");
        timeout_override = (unsigned)seconds + 15;
    }
    if (mount("sysfs", "/sys", "sysfs", 0, NULL)
        || mount("devtmpfs", "/dev", "devtmpfs", 0, NULL))
        fatal("mount pseudo-filesystems");
    /* Full-suite filesystem tests require disk-backed files (e.g. fs-verity).
     * QEMU snapshots isolate this ext4 root for every program. The selected
     * K7 contract keeps its original tmpfs; shared memory stays tmpfs in both. */
    if (!full_suite && mount("tmpfs", "/tmp", "tmpfs", 0, "mode=1777"))
        fatal("mount tmp");
    if (full_suite) {
        if (chmod("/tmp", 01777))
            fatal("set temporary directory mode");
        /* Namespace tests change /tmp's propagation type. That operation
         * needs a mountpoint, not just a directory in the root filesystem.
         * A self-bind supplies one without replacing ext4 or copying data. */
        if (mount("/tmp", "/tmp", NULL, MS_BIND, NULL))
            fatal("bind-mount disk-backed tmp");
        puts("KSELFTEST_SETUP /tmp ext4 (per-flight snapshot)");
    }
    if (mkdir("/dev/shm", 01777) && errno != EEXIST)
        fatal("mkdir shm");
    if (mount("tmpfs", "/dev/shm", "tmpfs", 0, "mode=1777"))
        fatal("mount shm");
    if (full_suite) {
        if ((mkdir("/dev/pts", 0755) && errno != EEXIST)
            || mount("devpts", "/dev/pts", "devpts", 0, "mode=0620,ptmxmode=0666"))
            fatal("mount devpts");
    }
    if (full_suite) {
        struct utsname kernel;
        char build[256];
        if (uname(&kernel))
            fatal("read running kernel release");
        snprintf(build, sizeof(build), "/lib/modules/%s/build", kernel.release);
        struct stat build_stat;
        if (!stat(build, &build_stat) && S_ISDIR(build_stat.st_mode)) {
            if (setenv("KBUILD_OUTPUT", build, 1))
                fatal("set module build directory");
            dprintf(1, "KSELFTEST_SETUP KBUILD_OUTPUT %s\n", build);
        }
        load_runtime_modules("/tests/modules", NULL);
        if (system("ip link set lo up"))
            fatal("enable loopback");
        const char *types[] = {"debugfs", "tracefs", "cgroup2", "bpf", "securityfs", "fusectl"};
        const char *paths[] = {"/sys/kernel/debug", "/sys/kernel/tracing", "/sys/fs/cgroup", "/sys/fs/bpf", "/sys/kernel/security", "/sys/fs/fuse/connections"};
        for (unsigned i = 0; i < sizeof(types) / sizeof(types[0]); ++i)
            if (mount(types[i], paths[i], types[i], 0, NULL))
                dprintf(1, "KSELFTEST_SETUP unavailable %s: %s\n", types[i], strerror(errno));
    }
    if (full_suite && access("/tests/kunit-modules", F_OK) == 0) {
        /* Boot with a nonmatching filter and autorun=1, then enable all
         * suites before loading modules. This includes init-only suites,
         * which cannot be run later through the debugfs run file. */
        file = fopen("/sys/module/kunit/parameters/filter_glob", "w");
        if (!file || fputs("*", file) < 0 || fclose(file))
            fatal("enable module KUnit suites");
        /* Console replay must not interleave with userspace boundaries.
         * This only changes console delivery; dmesg retains all test records. */
        if (system("dmesg --console-level 1"))
            fatal("select module log capture");
        unsigned count = load_runtime_modules("/tests/kunit-modules", nonce);
        if (!count)
            fatal("empty KUnit module list");
        dprintf(1, "KUNIT_MODULE_COMPLETE %s %u\n", nonce, count);
        count = run_kunit_suites("/tests/kunit-rerun", nonce, "/sys/kernel/debug/kunit");
        if (count)
            dprintf(1, "KUNIT_RERUN_COMPLETE %s %u\n", nonce, count);
        puts("EVO_INIT_DONE");
        poweroff();
    }
    if (symlink("/proc/self/fd", "/dev/fd") && errno != EEXIST)
        fatal("link fd");
    const char *streams[] = {"/dev/stdin", "/dev/stdout", "/dev/stderr"};
    for (unsigned i = 0; i < sizeof(streams) / sizeof(streams[0]); ++i) {
        char fd_path[32];
        snprintf(fd_path, sizeof(fd_path), "/proc/self/fd/%u", i);
        if (symlink(fd_path, streams[i]) && errno != EEXIST)
            fatal("link standard stream");
    }
    if (!full_suite && mount("/dev/vda", "/scratch", "ext4", 0, NULL))
        fatal("mount scratch");
    file = fopen("/tests/huge-pages", "r");
    unsigned pages, available;
    if (!file || fscanf(file, "%u", &pages) != 1)
        fatal("read huge-page setup");
    fclose(file);
    if (getenv("kselftest_hugepages") && (!full_suite || !strchr(collection, ':'))) {
        errno = EINVAL;
        fatal("huge-page override requires one full-suite program");
    }
    pages = huge_page_count(pages);
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
        if (collection[0] && strcmp(collection, id)) {
            char *colon = strchr(collection, ':');
            if (!colon || (size_t)(colon - collection) != strlen(id)
                || strncmp(collection, id, strlen(id)))
                continue;
        }
        const char *selected = collection[0] ? collection : id;
        dprintf(1, "KSELFTEST_BEGIN %s %s\n", nonce, selected);
        if (full_suite && (!strcmp(selected, "fpu") || !strncmp(selected, "fpu:", 4))) {
            long cpus = sysconf(_SC_NPROCESSORS_ONLN);
            if (cpus < 1 || cpus > 4096)
                fatal("read FPU worker CPU count");
            dprintf(1, "KSELFTEST_CPUS %s %s %ld\n", nonce, selected, cpus);
        }
        int rc = run(selected, timeout_override ? timeout_override : limit);
        /* A timed-out child may leave an unfinished progress line. */
        dprintf(1, "\nKSELFTEST_END %s %s %d\n", nonce, selected, rc);
        count++;
    }
    if (!feof(file) || !count)
        fatal("incomplete test list");
    fclose(file);
    puts("EVO_INIT_DONE");
    poweroff();
}
