/* Read-only QEMU API-7 diagnostic. Count entered instructions, not elapsed
 * time, in nonoverlapping symbol ranges supplied from the matching System.map.
 * Run with nokaslr; this is NOT a kernel acceptance or performance benchmark.
 * Per-vCPU scoreboards avoid a contended host atomic on every guest instruction.
 * Counts include a faulting instruction's entry and exclude unlisted addresses.
 */
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <qemu-plugin.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
struct region {
    uint64_t lo, hi;
    char name[256];
};
static struct region *regions;
static size_t count;
static struct qemu_plugin_scoreboard *counts;

static qemu_plugin_u64 counter(size_t index, size_t field)
{
    return (qemu_plugin_u64){counts, (index * 2 + field) * sizeof(uint64_t)};
}

static void translate(struct qemu_plugin_tb *tb, void *unused)
{
    (void)unused;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        size_t lo = 0, hi = count;
        while (lo < hi) {
            size_t mid = lo + (hi - lo) / 2;
            if (regions[mid].hi <= pc)
                lo = mid + 1;
            else
                hi = mid;
        }
        if (lo == count || pc < regions[lo].lo)
            continue;
        qemu_plugin_register_vcpu_insn_exec_inline_per_vcpu(
            insn, QEMU_PLUGIN_INLINE_ADD_U64, counter(lo, 0), 1);
        if (pc == regions[lo].lo)
            qemu_plugin_register_vcpu_insn_exec_inline_per_vcpu(
                insn, QEMU_PLUGIN_INLINE_ADD_U64, counter(lo, 1), 1);
    }
}

static void finished(void *unused)
{
    (void)unused;
    for (size_t i = 0; i < count; ++i) {
        uint64_t instructions = qemu_plugin_u64_sum(counter(i, 0));
        if (!instructions)
            continue;
        char line[512];
        snprintf(line, sizeof(line), "KVM_INSN %" PRIx64 " %" PRIu64
                 " %" PRIu64 " %s\n", regions[i].lo, instructions,
                 qemu_plugin_u64_sum(counter(i, 1)), regions[i].name);
        qemu_plugin_outs(line);
    }
    qemu_plugin_scoreboard_free(counts);
    free(regions);
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,
    const qemu_info_t *info, int argc, char **argv)
{
    if (argc != 1 || strncmp(argv[0], "ranges=", 7)
        || !info->system_emulation || strcmp(info->target_name, "aarch64"))
        return 1;
    FILE *input = fopen(argv[0] + 7, "r");
    if (!input)
        return 1;
    regions = calloc(100000, sizeof(*regions));
    if (!regions) {
        fclose(input);
        return 1;
    }
    struct region next;
    int fields;
    while ((fields = fscanf(input, "%" SCNx64 " %" SCNx64 " %255s",
                            &next.lo, &next.hi, next.name)) == 3) {
        if (count == 100000 || next.lo >= next.hi
            || (count && regions[count - 1].hi > next.lo)) {
            fclose(input);
            free(regions);
            return 1;
        }
        regions[count++] = next;
    }
    int valid = fields == EOF && !ferror(input) && count;
    fclose(input);
    if (!valid) {
        free(regions);
        return 1;
    }
    counts = qemu_plugin_scoreboard_new(2 * count * sizeof(uint64_t));
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finished, NULL);
    return 0;
}
