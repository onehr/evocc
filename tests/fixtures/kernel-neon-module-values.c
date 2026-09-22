// SPDX-License-Identifier: GPL-2.0-only
/* Reference-built test driver for the original XOR/RAID6 kernel modules. */
#include <linux/module.h>
#include <linux/raid/pq.h>
#include <linux/raid/xor.h>
#include <asm/neon.h>

extern struct xor_block_template xor_block_inner_neon;

#define DISKS 6
#define DATA (DISKS - 2)
static void *blocks[DISKS];
/* Supplied by the controller from this flight's /proc/kallsyms. The original
 * module does not export its individual algorithms; do not change its ELF. */
static unsigned long algorithms[4];
static unsigned algorithms_count;
module_param_array(algorithms, ulong, &algorithms_count, 0400);

static u8 sample(unsigned disk, unsigned byte)
{
    return (disk * 67 + byte * 13 + byte / 7) ^ (byte >> 4);
}

static u8 times_two(u8 value)
{
    return (value << 1) ^ ((value & 128) ? 0x1d : 0);
}

static void reset_data(size_t bytes)
{
    unsigned disk, byte;
    for (disk = 0; disk < DATA; ++disk)
        for (byte = 0; byte < bytes; ++byte)
            ((u8 *)blocks[disk])[byte] = sample(disk, byte);
    memset(blocks[DATA], 0xa5, bytes);
    memset(blocks[DATA + 1], 0x5a, bytes);
}

static bool data_matches(unsigned disk, size_t bytes)
{
    unsigned byte;
    for (byte = 0; byte < bytes; ++byte)
        if (((u8 *)blocks[disk])[byte] != sample(disk, byte))
            return false;
    return true;
}

static bool parity_matches(size_t bytes)
{
    unsigned byte;
    for (byte = 0; byte < bytes; ++byte) {
        u8 p = 0, q = 0;
        int disk;
        for (disk = DATA - 1; disk >= 0; --disk) {
            p ^= sample(disk, byte);
            q = times_two(q) ^ sample(disk, byte);
        }
        if (((u8 *)blocks[DATA])[byte] != p || ((u8 *)blocks[DATA + 1])[byte] != q)
            return false;
    }
    return true;
}

static bool xor_matches(unsigned count, size_t bytes)
{
    unsigned byte, disk;
    for (byte = 0; byte < bytes; ++byte) {
        u8 expected = sample(0, byte);
        for (disk = 1; disk <= count; ++disk)
            expected ^= sample(disk, byte);
        if (((u8 *)blocks[0])[byte] != expected)
            return false;
    }
    return true;
}

static int __init check_values(void)
{
    static const size_t sizes[] = {256, 1024, 4096};
    unsigned disk, size, count, variant, passed = 0;
    struct raid6_calls original = raid6_call;
    int rc = -ENOMEM;
    for (disk = 0; disk < DISKS; ++disk) {
        blocks[disk] = (void *)__get_free_page(GFP_KERNEL);
        if (!blocks[disk])
            goto done;
    }
    rc = -EINVAL;
    pr_info("NEON_VALUES raid6 selected %s\n", raid6_call.name);
    if (algorithms_count != ARRAY_SIZE(algorithms))
        goto done;
    for (size = 0; size < ARRAY_SIZE(sizes); ++size) {
        size_t bytes = sizes[size];
        for (count = 1; count <= 4; ++count) {
            /* Four direct calls prevent a faster scalar XOR choice from
             * hiding the NEON routines. Each call has its proper signature. */
            for (disk = 0; disk <= count; ++disk) {
                unsigned byte;
                for (byte = 0; byte < bytes; ++byte)
                    ((u8 *)blocks[disk])[byte] = sample(disk, byte);
            }
            kernel_neon_begin();
            switch (count) {
            case 1: xor_block_inner_neon.do_2(bytes, blocks[0], blocks[1]); break;
            case 2: xor_block_inner_neon.do_3(bytes, blocks[0], blocks[1], blocks[2]); break;
            case 3: xor_block_inner_neon.do_4(bytes, blocks[0], blocks[1], blocks[2], blocks[3]); break;
            case 4: xor_block_inner_neon.do_5(bytes, blocks[0], blocks[1], blocks[2], blocks[3], blocks[4]); break;
            }
            kernel_neon_end();
            if (!xor_matches(count, bytes))
                goto done;
            ++passed;
            for (disk = 0; disk < bytes; ++disk)
                ((u8 *)blocks[0])[disk] = sample(0, disk);
            xor_blocks(count, bytes, blocks[0], &blocks[1]);
            if (!xor_matches(count, bytes))
                goto done;
            ++passed;
        }
        for (variant = 0; variant < ARRAY_SIZE(algorithms); ++variant) {
            const struct raid6_calls *algorithm = (const void *)algorithms[variant];
            if (!algorithm || strncmp(algorithm->name, "neon", 4)
                || (algorithm->valid && !algorithm->valid()))
                goto done;
            raid6_call = *algorithm;
            reset_data(bytes);
            raid6_call.gen_syndrome(DISKS, bytes, blocks);
            if (!parity_matches(bytes))
                goto done;
            ++passed;
            for (disk = 0; disk < DATA; ++disk) {
                unsigned second;
                for (second = disk + 1; second < DATA; ++second) {
                    reset_data(bytes);
                    raid6_call.gen_syndrome(DISKS, bytes, blocks);
                    memset(blocks[disk], 0, bytes);
                    memset(blocks[second], 0, bytes);
                    raid6_2data_recov(DISKS, bytes, disk, second, blocks);
                    if (!data_matches(disk, bytes) || !data_matches(second, bytes) || !parity_matches(bytes))
                        goto done;
                    ++passed;
                }
                reset_data(bytes);
                raid6_call.gen_syndrome(DISKS, bytes, blocks);
                memset(blocks[disk], 0, bytes);
                memset(blocks[DATA], 0, bytes);
                raid6_datap_recov(DISKS, bytes, disk, blocks);
                if (!data_matches(disk, bytes) || !parity_matches(bytes))
                    goto done;
                ++passed;
            }
            pr_info("NEON_VALUES verified %s bytes=%zu\n", algorithm->name, bytes);
        }
    }
    rc = 0;
done:
    raid6_call = original;
    for (disk = 0; disk < DISKS; ++disk)
        if (blocks[disk])
            free_page((unsigned long)blocks[disk]);
    pr_info("NEON_VALUES %s %u/156 byte-checked operations\n", rc ? "FAIL" : "PASS", passed);
    return rc;
}

static void __exit leave(void) {}
module_init(check_values);
module_exit(leave);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Byte-exact XOR and RAID6 module verification");
