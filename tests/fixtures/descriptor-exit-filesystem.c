/* Reduce libc-test daemon-failure to descriptor duplication and child exit.
 * No daemon call, libc replacement, directory change or descriptor-limit change.
 * Run stdout on a guest-local file and then on the host virtiofs mount.
 */
#include <errno.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    int child = fork();
    if (child < 0) return 1;
    if (!child) {
        unsigned count = 0;
        while (dup(1) >= 0) ++count;
        int exhausted = errno == EMFILE;
        fprintf(stderr, "before exit: duplicates=%u exhausted=%d\n", count, exhausted);
        _Exit(!exhausted);
    }
    int status;
    if (waitpid(child, &status, 0) != child) return 2;
    return !WIFEXITED(status) || WEXITSTATUS(status);
}
