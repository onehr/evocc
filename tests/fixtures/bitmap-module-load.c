/* Original module test on one matching reference Image; this is not a
 * uniform EvoCC-kernel claim. Metadata and host controller stay reference-built. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/klog.h>
#include <sys/syscall.h>
#include <unistd.h>

static int number, failures;
static void check(int ok, const char *name)
{
    printf("%s %d %s (errno=%d)\n", ok ? "ok" : "not ok", ++number, name, ok ? 0 : errno);
    failures += !ok;
    fflush(stdout);
}
static unsigned report(void)
{
    char log[65536];
    int size = klogctl(3, log, sizeof(log) - 1);
    if (size < 0) return 0;
    log[size] = 0;
    unsigned total = 0, summaries = 0;
    int failed = 0;
    for (char *line = strtok(log, "\n"); line; line = strtok(NULL, "\n")) {
        char *message = strstr(line, "test_bitmap: ");
        if (!message) continue;
        printf("# %s\n", message);
        failed |= strstr(message, "failed") != NULL || strstr(message, "skipped") != NULL;
        unsigned n;
        if (sscanf(message, "test_bitmap: all %u tests passed", &n) == 1) {
            summaries++;
            total = n;
        }
    }
    return !failed && summaries == 1 ? total : 0;
}
int main(void)
{
    const char *paths[] = {"/tests/reference-test_bitmap.ko", "/tests/subject-test_bitmap.ko"};
    unsigned totals[2] = {0, 0};
    puts("TAP version 13\n1..7");
    for (unsigned i = 0; i < 2; i++) {
        printf("# module=%s\n", paths[i]);
        int cleared = klogctl(5, NULL, 0) == 0;
        int fd = open(paths[i], O_RDONLY);
        int loaded = cleared && fd >= 0 && syscall(SYS_finit_module, fd, "", 0) == 0;
        if (fd >= 0) close(fd);
        check(loaded, "matching signed original bitmap module loads");
        totals[i] = report();
        check(loaded && totals[i] != 0, "original bitmap selftest reports a nonempty all-pass summary");
        check(loaded && syscall(SYS_delete_module, "test_bitmap", O_NONBLOCK) == 0, "bitmap module unloads");
    }
    check(totals[0] != 0 && totals[0] == totals[1], "reference and subject execute the same test count");
    return failures != 0;
}
