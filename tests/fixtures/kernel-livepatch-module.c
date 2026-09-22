/* Execute the frozen upstream livepatch sample; the module is the subject. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

static int cmdline(char *buffer, size_t size)
{
    FILE *file = fopen("/proc/cmdline", "r");
    if (!file)
        return -1;
    int ok = fgets(buffer, size, file) != NULL;
    fclose(file);
    return ok ? 0 : -1;
}

static int await_cmdline(const char *expected)
{
    char observed[4096];
    for (int attempt = 0; attempt < 500; ++attempt) {
        if (!cmdline(observed, sizeof(observed)) && !strcmp(observed, expected))
            return 0;
        usleep(10000);
    }
    return -1;
}

static int result(int number, const char *name, int success)
{
    printf("%s %d - %s (errno=%d)\n", success ? "ok" : "not ok", number, name, errno);
    fflush(stdout);
    return success;
}

int main(void)
{
    char original[4096];
    if (cmdline(original, sizeof(original))) {
        puts("Bail out! original cmdline unavailable");
        return 1;
    }
    puts("TAP version 13\n1..5");
    int fd = open("/tests/livepatch-sample.ko", O_RDONLY);
    int loaded = fd >= 0 && syscall(SYS_finit_module, fd, "", 0) == 0;
    if (fd >= 0)
        close(fd);
    if (!result(1, "load original livepatch sample", loaded))
        return 1;
    int patched = !await_cmdline("this has been live patched\n");
    result(2, "replacement function executes", patched);
    fd = open("/sys/kernel/livepatch/livepatch_sample/enabled", O_WRONLY);
    int disabled = fd >= 0 && write(fd, "0\n", 2) == 2;
    if (fd >= 0)
        close(fd);
    result(3, "disable patch", disabled);
    int restored = disabled && !await_cmdline(original);
    result(4, "original function restored", restored);
    int unloaded = 0;
    if (restored) {
        for (int attempt = 0; attempt < 500; ++attempt) {
            if (!syscall(SYS_delete_module, "livepatch_sample", O_NONBLOCK)) {
                unloaded = 1;
                break;
            }
            if (errno != EWOULDBLOCK && errno != EBUSY)
                break;
            usleep(10000);
        }
    }
    result(5, "unload patch module", unloaded);
    return !(loaded && patched && disabled && restored && unloaded);
}
