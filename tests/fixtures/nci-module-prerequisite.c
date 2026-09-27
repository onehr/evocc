/* Diagnostic-only NCI prerequisite. Modules and original userspace must match
 * the selected reference Image; no rebuilt selftest or relaxed assertion. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures, count;

static void check(int ok, const char *name)
{
    printf("%s %d %s\n", ok ? "ok" : "not ok", ++count, name);
    failures += !ok;
    fflush(stdout);
}

static int load(const char *path)
{
    int fd = open(path, O_RDONLY);
    int ok = fd >= 0 && syscall(SYS_finit_module, fd, "", 0) == 0;
    printf("# load %s ok=%d errno=%d\n", path, ok, ok ? 0 : errno);
    if (fd >= 0)
        close(fd);
    return ok;
}

static int original(const char *phase)
{
    const char *log = "/tmp/nci-original.log";
    int fd = open(log, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0)
        return 127;
    fflush(stdout);
    pid_t child = fork();
    if (!child) {
        if (dup2(fd, 1) < 0 || dup2(fd, 2) < 0)
            _exit(127);
        close(fd);
        alarm(45); /* Same original upstream program bound. */
        execl("/tests/nci_dev", "nci_dev", (char *)0);
        _exit(127);
    }
    int status = 0;
    int rc = child > 0 && waitpid(child, &status, 0) == child
        ? (WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status)) : 127;
    close(fd);
    FILE *file = fopen(log, "r");
    char line[4096];
    printf("# NCI_ORIGINAL_BEGIN %s\n", phase);
    if (file) {
        while (fgets(line, sizeof(line), file))
            printf("# nci_original | %s", line);
        if (ferror(file))
            rc = 127;
        fclose(file);
    } else {
        rc = 127;
    }
    printf("# NCI_ORIGINAL_END %s %d\n", phase, rc);
    return rc;
}

int main(void)
{
    struct stat st;
    printf("TAP version 13\n1..11\n");
    check(load("/tests/rfkill.ko"), "load signed RF-kill dependency");
    check(load("/tests/nfc.ko"), "load signed NFC dependency");
    check(load("/tests/nci.ko"), "load signed NCI dependency");
    errno = 0;
    int fd = open("/dev/virtual_nci", O_RDWR);
    check(fd == -1 && errno == ENOENT, "device absent before matching driver load");
    if (fd >= 0)
        close(fd);
    check(original("before") == 1, "unchanged original fails before virtual driver load");
    check(load("/tests/virtual_ncidev.ko"), "load signed virtual driver");
    check(stat("/dev/virtual_nci", &st) == 0 && S_ISCHR(st.st_mode),
          "driver registers character device without a userspace daemon");
    fd = open("/dev/virtual_nci", O_RDWR);
    unsigned index = ~0U;
    int ok = fd >= 0 && ioctl(fd, 0, &index) == 0 && index != ~0U;
    check(ok, "open and query a real virtual NCI instance");
    if (fd >= 0)
        close(fd);
    check(original("after") == 0,
          "unchanged eight-case NCI program completes successfully");
    check(syscall(SYS_delete_module, "virtual_ncidev", O_NONBLOCK) == 0
          && stat("/dev/virtual_nci", &st) == -1 && errno == ENOENT,
          "unload removes the device again");
    check(syscall(SYS_delete_module, "nci", O_NONBLOCK) == 0
          && syscall(SYS_delete_module, "nfc", O_NONBLOCK) == 0
          && syscall(SYS_delete_module, "rfkill", O_NONBLOCK) == 0,
          "dependencies unload after original tests release their instances");
    return failures != 0;
}
