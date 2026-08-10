// Flight-lab reference init.
//
// This is the PID 1 payload for Day-1 reference kernel boot flights. It is
// compiled with the sealed GCC toolchain and packed into a minimal initramfs;
// it exists to make the boot predicate mechanical rather than narrative.
//
// Ordered boot predicate contract:
//   1. emit "EVO_INIT_ALIVE"
//   2. mount /proc, read /proc/cmdline, extract the evo_nonce= value injected
//      by the flight harness, emit "EVO_BOOT_NONCE <nonce>"
//   3. emit "EVO_INIT_DONE"
//   4. power off the machine
//
// Every marker is emitted as a single write(2) so concurrent kernel printk
// traffic can interleave between markers but never inside one; the boot
// harness matches markers as exact substrings and checks their byte order.
//
// A replayed serial log from an earlier flight fails because the nonce will
// not match the current flight's nonce. A fixed marker without the nonce
// fails because marker 2 must carry the exact injected value. A wrong rootfs
// fails because this init is not present. On Day 27 this file's role is taken
// over by an EvoCC-compiled init; until then a GCC-built init is the honest
// reference witness.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mount.h>
#include <sys/reboot.h>
#include <linux/reboot.h>

#define CMDLINE_PATH "/proc/cmdline"
#define NONCE_KEY "evo_nonce="
#define MAX_NONCE 128

/* One marker, one syscall: the unit the boot harness can trust. */
static void emit(const char *marker)
{
    write(STDOUT_FILENO, marker, strlen(marker));
}

static int extract_nonce(char *out, size_t out_size)
{
    /* PID 1 starts with no pseudo-filesystems; /proc must be mounted before
     * /proc/cmdline exists. The directory is provided by the initramfs. */
    if (mount("proc", "/proc", "proc", 0, NULL) != 0)
        return -1;

    FILE *f = fopen(CMDLINE_PATH, "r");
    if (!f)
        return -1;

    char buf[4096];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = '\0';

    char *p = strstr(buf, NONCE_KEY);
    if (!p)
        return -1;
    p += strlen(NONCE_KEY);

    size_t i = 0;
    while (i < out_size - 1 && p[i] && p[i] != ' ' && p[i] != '\n' && p[i] != '\t') {
        out[i] = p[i];
        i++;
    }
    out[i] = '\0';
    return i > 0 ? 0 : -1;
}

int main(void)
{
    static char nonce[MAX_NONCE];
    char line[256];

    emit("EVO_INIT_ALIVE\n");

    if (extract_nonce(nonce, sizeof(nonce)) != 0) {
        /* A missing nonce is a predicate failure, not a silent pass. */
        emit("EVO_BOOT_NONCE_MISSING\n");
        emit("EVO_INIT_ABORT\n");
        sync();
        reboot(LINUX_REBOOT_CMD_POWER_OFF);
        return 1;
    }

    int len = snprintf(line, sizeof(line), "EVO_BOOT_NONCE %s\n", nonce);
    write(STDOUT_FILENO, line, (size_t)len);
    emit("EVO_INIT_DONE\n");

    sync();
    reboot(LINUX_REBOOT_CMD_POWER_OFF);

    /* reboot(2) only returns on failure; loop rather than falling off. */
    for (;;)
        pause();
}
