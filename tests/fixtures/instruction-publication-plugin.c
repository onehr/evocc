/* Read-only QEMU 11.1/API-7 diagnostic, compiled for the host, not EvoCC.
 * Observe actual guest store callbacks and entry into immutable return targets.
 * The instruction-data API rereads guest bytes at translation completion, so
 * its sampled word is NOT proof of the word the translator originally decoded.
 * No cache flush, register/memory mutation or result waiver.
 * Only the fixed-address reduction's two writer functions are instrumented;
 * its rwlock keeps the publication flag stable during each entry callback.
 * A target witness localizes stale control flow; it does not by itself locate
 * the invalidation race or prove that the original sched_ext fault is identical. */
#include <inttypes.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <qemu-plugin.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static uint64_t publication_address, writer_lo[2], writer_hi[2];
static uint64_t store_pc;
static atomic_uint publication, code_word;
static atomic_ulong stores, executions, witnesses, translations, entry_samples;
static atomic_ulong store_started, store_completed, window_witnesses;
struct translated {
    uint64_t address;
    uint32_t word;
    int target;
    unsigned long started, completed;
};
/* Per-vCPU, not per host thread: single-thread TCG interleaves guest CPUs. */
static const struct translated **last_entry;
static unsigned cpu_count;

static void begin_store(unsigned cpu, void *unused)
{
    (void)cpu;
    (void)unused;
    atomic_fetch_add(&store_started, 1);
}

static void observe_store(unsigned cpu, qemu_plugin_meminfo_t info,
                          uint64_t address, void *unused)
{
    (void)cpu;
    (void)unused;
    if (address != publication_address && address != UINT64_C(0x3000000000))
        return;
    qemu_plugin_mem_value value = qemu_plugin_mem_get_value(info);
    if (value.type != QEMU_PLUGIN_MEM_VALUE_U32)
        return;
    atomic_store(address == publication_address ? &publication : &code_word,
                 value.data.u32);
    if (address == UINT64_C(0x3000000000) && store_pc)
        atomic_store(&store_completed, atomic_load(&store_started));
    atomic_fetch_add(&stores, 1);
}

static void observe_entry(unsigned cpu, void *opaque)
{
    const struct translated *instruction = opaque;
    atomic_fetch_add(&executions, 1);
    if (store_pc && cpu < cpu_count && !instruction->target)
        last_entry[cpu] = instruction;
    if (!instruction->target && instruction->word != UINT32_C(0x14000003))
        return;
    unsigned flag = atomic_load(&publication);
    unsigned stored = atomic_load(&code_word);
    if (!flag && stored == UINT32_C(0x14000001)) {
        if (!instruction->target) {
            atomic_fetch_add(&entry_samples, 1);
            return;
        }
        unsigned long index = atomic_fetch_add(&witnesses, 1);
        const struct translated *origin = cpu < cpu_count ? last_entry[cpu] : NULL;
        int window = store_pc && origin && origin->started > origin->completed;
        if (window)
            atomic_fetch_add(&window_witnesses, 1);
        if (index < 32) {
            char line[384];
            snprintf(line, sizeof(line), "K01_TB_TARGET index=%lu cpu=%u pc=%" PRIx64
                    " sampled=%08x stored=%08x published=%u stores=%lu"
                    " origin=%" PRIx64 " origin_word=%08x translation_store=%lu/%lu current_store=%lu/%lu\n",
                    index, cpu, instruction->address, instruction->word,
                    stored, flag, atomic_load(&stores),
                    origin ? origin->address : 0, origin ? origin->word : 0,
                    origin ? origin->started : 0, origin ? origin->completed : 0,
                    atomic_load(&store_started), atomic_load(&store_completed));
            qemu_plugin_outs(line);
        }
    }
}

static void translate(struct qemu_plugin_tb *tb, void *unused)
{
    (void)unused;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t address = qemu_plugin_insn_vaddr(insn);
        int entry = address == UINT64_C(0x3000000000) || address == UINT64_C(0x3000100000);
        int target = address == UINT64_C(0x300000000c) || address == UINT64_C(0x300010000c);
        if (entry || target) {
            struct translated *copy = g_new0(struct translated, 1);
            copy->address = address;
            copy->target = target;
            copy->started = atomic_load(&store_started);
            copy->completed = atomic_load(&store_completed);
            if (qemu_plugin_insn_data(insn, &copy->word, sizeof(copy->word)) != 4) {
                g_free(copy);
                continue;
            }
            atomic_fetch_add(&translations, 1);
            qemu_plugin_register_vcpu_insn_exec_cb(insn, observe_entry,
                                                  QEMU_PLUGIN_CB_NO_REGS, copy);
            /* Callback userdata lives until QEMU exit; no TB lifetime hook is
             * exposed. The bounded 512-round reduction limits this allocation. */
        }
        if (store_pc && address == store_pc)
            qemu_plugin_register_vcpu_insn_exec_cb(insn, begin_store,
                                                  QEMU_PLUGIN_CB_NO_REGS, NULL);
        if ((address >= writer_lo[0] && address < writer_hi[0])
            || (address >= writer_lo[1] && address < writer_hi[1]))
            qemu_plugin_register_vcpu_mem_cb(insn, observe_store,
                QEMU_PLUGIN_CB_NO_REGS, QEMU_PLUGIN_MEM_W, NULL);
    }
}

static void finished(void *unused)
{
    (void)unused;
    char line[256];
    snprintf(line, sizeof(line), "K01_TB_TOTAL stores=%lu translations=%lu executions=%lu target_witnesses=%lu entry_samples=%lu window_witnesses=%lu store_started=%lu store_completed=%lu\n",
            atomic_load(&stores), atomic_load(&translations),
            atomic_load(&executions), atomic_load(&witnesses), atomic_load(&entry_samples),
            atomic_load(&window_witnesses), atomic_load(&store_started), atomic_load(&store_completed));
    qemu_plugin_outs(line);
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,
    const qemu_info_t *info, int argc, char **argv)
{
    if ((argc != 5 && argc != 6) || !info->system_emulation || strcmp(info->target_name, "aarch64"))
        return 1;
    uint64_t *fields[] = {&publication_address, &writer_lo[0], &writer_hi[0],
                         &writer_lo[1], &writer_hi[1], &store_pc};
    const char *keys[] = {"pub=", "mainlo=", "mainhi=", "patchlo=", "patchhi=", "storepc="};
    for (int i = 0; i < argc; ++i) {
        char *end;
        size_t length = strlen(keys[i]);
        if (strncmp(argv[i], keys[i], length)) return 1;
        const char *value = argv[i] + length;
        *fields[i] = strtoull(value, &end, 16);
        if (!*value || *end) return 1;
    }
    cpu_count = info->system.max_vcpus;
    last_entry = g_new0(const struct translated *, cpu_count);
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finished, NULL);
    return 0;
}
