/* User-space controller; the original kernel modules are the compiler subject. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/klog.h>
#include <sys/syscall.h>
#include <unistd.h>

static int parameters(char *output, size_t size)
{
    static const char *const names[] = {"raid6_neonx1", "raid6_neonx2", "raid6_neonx4", "raid6_neonx8"};
    unsigned long addresses[4] = {0};
    char line[1024], symbol[256], owner[256], kind;
    FILE *file = fopen("/proc/kallsyms", "r");
    if (!file)
        return -1;
    while (fgets(line, sizeof(line), file)) {
        unsigned long address;
        if (sscanf(line, "%lx %c %255s %255s", &address, &kind, symbol, owner) != 4
            || strcmp(owner, "[raid6_pq]"))
            continue;
        for (int i = 0; i < 4; ++i)
            if (!strcmp(symbol, names[i])) {
                if (addresses[i] || !address) {
                    fclose(file);
                    return -1;
                }
                addresses[i] = address;
            }
    }
    fclose(file);
    for (int i = 0; i < 4; ++i)
        if (!addresses[i])
            return -1;
    int length = snprintf(output, size, "algorithms=0x%lx,0x%lx,0x%lx,0x%lx", addresses[0], addresses[1], addresses[2], addresses[3]);
    return length < 0 || (size_t)length >= size ? -1 : 0;
}

static int value_results(void)
{
    char log[65536];
    int length = klogctl(3, log, sizeof(log) - 1);
    if (length < 0)
        return 0;
    log[length] = 0;
    unsigned variants = 0, passed = 0;
    for (char *line = strtok(log, "\n"); line; line = strtok(NULL, "\n")) {
        if (strstr(line, "NEON_VALUES"))
            printf("# %s\n", line);
        variants += strstr(line, "NEON_VALUES verified neonx") != NULL;
        passed += strstr(line, "NEON_VALUES PASS 156/156 byte-checked operations") != NULL;
    }
    return variants == 12 && passed == 1;
}

int main(void)
{
    static const char *const files[] = {"xor-neon", "xor", "raid6_pq", "neon-values"};
    static const char *const names[] = {"xor_neon", "xor", "raid6_pq", "neon_values"};
    int failed = 0, loaded = 0, number = 0;
    puts("TAP version 13\n1..8");
    for (int i = 0; i < 4; ++i) {
        char path[128];
        char params[256] = "";
        int ready = i != 3 || !parameters(params, sizeof(params));
        snprintf(path, sizeof(path), "/tests/%s.ko", files[i]);
        int fd = open(path, O_RDONLY);
        int inserted = !failed && ready && fd >= 0 && syscall(SYS_finit_module, fd, params, 0) == 0;
        int ok = inserted && (i != 3 || value_results());
        int error = errno;
        if (fd >= 0)
            close(fd);
        printf("%s %d - load %s%s (errno=%d)\n", ok ? "ok" : "not ok", ++number,
               files[i], i == 3 ? " and check 156 data operations" : "", ok ? 0 : error);
        fflush(stdout);
        if (inserted)
            ++loaded;
        if (!ok)
            failed = 1;
    }
    for (int i = 3; i >= 0; --i) {
        int ok = i < loaded && syscall(SYS_delete_module, names[i], O_NONBLOCK) == 0;
        printf("%s %d - unload %s (errno=%d)\n", ok ? "ok" : "not ok", ++number, names[i], ok ? 0 : errno);
        fflush(stdout);
        failed |= !ok;
    }
    return failed;
}
