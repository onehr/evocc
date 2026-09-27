/* B08d: bounded observations of the pinned library and test environment.
 * CASE=0..5. These do not replace the unchanged upstream tests or make
 * their failures PASS. Numeric values/flags are captured before printing. */
#define _GNU_SOURCE
#include <errno.h>
#include <fenv.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include <wordexp.h>
#ifndef CASE
#define CASE 0
#endif
#ifndef DROP_PRIVILEGE
#define DROP_PRIVILEGE 0
#endif
#ifndef TEST_NOFILE
#define TEST_NOFILE 0
#endif

#if CASE == 1
static void bits(long double x)
{
    unsigned char bytes[sizeof x];
    memcpy(bytes, &x, sizeof x);
    for (unsigned i = sizeof x; i; --i) printf("%02x", bytes[i-1]);
}
#endif

int main(void)
{
#if CASE == 0
    struct tm tm = {0};
    char *end = strptime("683078400", "%s", &tm);
    printf("epoch: consumed=%d y=%d m=%d d=%d\n", end && !*end,
           tm.tm_year, tm.tm_mon, tm.tm_mday);
    const char *inputs[] = {"-06", "-0600"};
    for (unsigned i = 0; i < 2; ++i) {
        memset(&tm, 0, sizeof tm);
        end = strptime(inputs[i], "%z", &tm);
        printf("zone %s: consumed=%d offset=%ld\n", inputs[i],
               end && !*end, tm.tm_gmtoff);
    }
#elif CASE == 1
    const char *text[] = {"0x1.111111111111111111111111111281",
                          "0x1.11111111111111111111111111111"};
    const long double expected[] = {0x1.1111111111111111111111111113p0L,
                                   0x1.1111111111111111111111111111p0L};
    if (fesetround(FE_TONEAREST)) return 100;
    for (unsigned i = 0; i < 2; ++i) {
        char *end;
        long double value = strtold(text[i], &end);
        printf("quad %u: consumed=%d equal=%d expected=", i, !*end,
               value == expected[i]);
        bits(expected[i]); printf(" actual="); bits(value); printf("\n");
    }
#elif CASE == 2
    volatile float x = 0x1.fffffep127f, y = 1;
    float (*volatile function)(float, float) = powf;
    if (fesetround(FE_UPWARD) || feclearexcept(FE_ALL_EXCEPT)) return 100;
    float value = function(x, y);
    int flags = fetestexcept(FE_ALL_EXCEPT);
    uint32_t representation;
    memcpy(&representation, &value, sizeof representation);
    printf("powf: bits=%08x flags=%x overflow=%d inexact=%d\n",
           representation, flags, !!(flags & FE_OVERFLOW), !!(flags & FE_INEXACT));
#elif CASE == 3
    const char *text[] = {"$#", "$1", "$2"};
    for (unsigned i = 0; i < 3; ++i) {
        wordexp_t words = {0};
        int error = wordexp(text[i], &words, 0);
        printf("wordexp %s: error=%d count=%zu", text[i], error, words.we_wordc);
        if (!error) {
            for (unsigned j = 0; j < words.we_wordc; ++j)
                printf(" [%s]", words.we_wordv[j]);
            wordfree(&words);
        }
        printf("\n");
    }
#elif CASE == 4
    if (DROP_PRIVILEGE && (setgid(65534) || setuid(65534))) return 100;
    struct rlimit limit = {0, 0};
    if (setrlimit(RLIMIT_NPROC, &limit)) return 101;
    errno = 0;
    pid_t pid = fork();
    int error = errno;
    if (!pid) _exit(0);
    if (pid > 0) {
        int status;
        while (waitpid(pid, &status, 0) < 0) if (errno != EINTR) return 102;
    }
    printf("fork: uid=%lu success=%d errno=%d\n", (unsigned long)getuid(),
           pid > 0, error);
#elif CASE == 5
    struct rlimit limit;
    if (getrlimit(RLIMIT_NOFILE, &limit)) return 100;
    if (TEST_NOFILE) {
        limit.rlim_cur = TEST_NOFILE;
        if (setrlimit(RLIMIT_NOFILE, &limit)) return 101;
    }
    struct timespec start, now;
    if (clock_gettime(CLOCK_MONOTONIC, &start)) return 102;
    unsigned count = 0;
    int error = 0;
    long long ns = 0;
    for (;;) {
        if (dup(STDOUT_FILENO) == -1) { error = errno; break; }
        ++count;
        if (!(count % 1024)) {
            if (clock_gettime(CLOCK_MONOTONIC, &now)) return 103;
            ns = (now.tv_sec - start.tv_sec) * 1000000000LL + now.tv_nsec - start.tv_nsec;
            if (ns >= 2000000000LL) break;
        }
    }
    printf("fill: limit=%llu count=%u errno=%d elapsed_ns=%lld\n",
           (unsigned long long)limit.rlim_cur, count, error, ns);
#else
#error unsupported CASE
#endif
    return 0;
}
