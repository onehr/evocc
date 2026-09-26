/* K12 diagnostic: missing test_module is a runtime prerequisite, not a
 * splice value mismatch. The module must match the booted Image and signer.
 * This reference-built observer neither contributes EvoCC object credit nor
 * changes the unchanged upstream shell test or its five-second bound. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures, number;

static void check(int ok, const char *name)
{
    printf("%s %d %s\n", ok ? "ok" : "not ok", ++number, name);
    failures += !ok;
    fflush(stdout);
}

static int matches(const char *path, size_t limit)
{
    char ordinary[4096], transferred[4096];
    int fd = open(path, O_RDONLY), pipefd[2];
    if (fd < 0)
        return 0;
    ssize_t expected = read(fd, ordinary, sizeof(ordinary));
    close(fd);
    if (expected <= 0 || pipe(pipefd))
        return 0;
    if ((size_t)expected > limit)
        expected = (ssize_t)limit;
    fd = open(path, O_RDONLY);
    ssize_t copied = fd < 0 ? -1 : splice(fd, NULL, pipefd[1], NULL, limit, 0);
    int error = errno;
    close(pipefd[1]);
    ssize_t received = read(pipefd[0], transferred, sizeof(transferred));
    close(pipefd[0]);
    if (fd >= 0)
        close(fd);
    printf("# %s limit=%zu splice=%zd errno=%d read=%zd expected=%zd\n",
           path, limit, copied, copied < 0 ? error : 0, received, expected);
    return copied == expected && received == expected
        && !memcmp(ordinary, transferred, (size_t)expected);
}

int main(void)
{
    const char *attribute = "/sys/module/test_module/coresize";
    const char *binary = "/sys/module/test_module/sections/.init.text";
    struct stat st;
    printf("TAP version 13\n1..8\n");
    errno = 0;
    check(stat(attribute, &st) == -1 && errno == ENOENT,
          "unloaded test module has no sysfs object");
    int fd = open("/tests/test_module.ko", O_RDONLY);
    int loaded = fd >= 0 && syscall(SYS_finit_module, fd, "", 0) == 0;
    printf("# finit_module loaded=%d errno=%d\n", loaded, loaded ? 0 : errno);
    if (fd >= 0)
        close(fd);
    check(loaded, "matching signed module actually loads");
    check(loaded && matches(attribute, 4096), "attribute full splice matches read");
    check(loaded && matches(attribute, 2), "attribute two-byte splice matches read");
    check(loaded && matches(binary, 4096), "binary attribute full splice matches read");
    check(loaded && matches(binary, 2), "binary attribute two-byte splice matches read");
    pid_t child = loaded ? fork() : -1;
    if (child == 0) {
        alarm(5); /* original splice/settings limit */
        execl("/bin/sh", "sh", "/tests/splice/short_splice_read.sh", (char *)0);
        _exit(127);
    }
    int status = 0;
    check(child > 0 && waitpid(child, &status, 0) == child
          && WIFEXITED(status) && WEXITSTATUS(status) == 0,
          "unchanged original shell test exits zero within five seconds");
    check(loaded && syscall(SYS_delete_module, "test_module", O_NONBLOCK) == 0
          && stat(attribute, &st) == -1 && errno == ENOENT,
          "module unload removes the sysfs prerequisite again");
    return failures != 0;
}
