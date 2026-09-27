/* B06: probe the real environment required by unmodified upstream tests.
 * Run only in the disposable test container. 77 is unavailable, never PASS. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void)
{
    int missing = 0;
    const char *user = getenv("USER");
    printf("USER=%s uid=%lu\n", user ? user : "<unset>", (unsigned long)getuid());
    if (!user || !*user) missing = 1;
    char hostname[256];
    if (gethostname(hostname, sizeof(hostname))) return 2;
    if (sethostname(hostname, strlen(hostname))) {
        printf("sethostname errno=%d\n", errno);
        missing = 1;
    } else puts("sethostname same-name PASS");
    char directory[] = "/tmp/tb-mount.XXXXXX";
    if (!mkdtemp(directory)) return 2;
    if (mount("none", directory, "tmpfs", MS_RDONLY, "size=4096")) {
        printf("mount readonly tmpfs errno=%d\n", errno);
        missing = 1;
    } else {
        char absent[128];
        snprintf(absent, sizeof(absent), "%s/missing", directory);
        struct stat st;
        int found = stat(absent, &st), error = errno;
        if (umount(directory)) return 2;
        if (found != -1 || error != ENOENT) return 1;
        puts("readonly missing-file stat ENOENT PASS");
    }
    if (rmdir(directory)) return 2;
    return missing ? 77 : 0;
}
