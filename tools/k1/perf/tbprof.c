/* Per-block execution profile for QEMU system emulation (plugin API 7).
 *
 * Each distinct translation block (guest PC, instruction count and code
 * words) gets one per-vCPU counter. An inline add increments it on every
 * block entry. Instruction totals are block entries times block length, so
 * a block that stops at a faulting instruction still counts its full length.
 * A marker PC writes a snapshot of every changed counter when a block that
 * starts at that PC executes. The plugin reads guest code and never writes
 * guest state.
 *
 * Arguments: out=PREFIX (required), mark=PC (hexadecimal, repeatable, at
 * most 16), capacity=BLOCKS (default 2097152), discon=off (no exception or
 * interrupt counts).
 * Output: PREFIX.blocks  index, PC, length and code words of each block
 *         PREFIX.snaps   snapshot headers, then changed counters
 *         PREFIX.discon  exception/interrupt counts by type and target PC
 */
#include <inttypes.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <qemu-plugin.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;

enum { MAX_MARKS = 16, MAX_INSNS = 512, DISCON_SLOTS = 1 << 14 };

struct block {
    uint64_t pc, key;
    uint32_t n, code;
};

struct discon {
    uint64_t to_pc, count;
    uint32_t type, from_high;
};

static struct block *blocks;
static uint64_t *table, *last;
static uint32_t *pool;
static size_t capacity, used, mask, pool_used, pool_cap;
static uint64_t dropped, irregular, discon_dropped;
static struct qemu_plugin_scoreboard *score;
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t snap_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t discon_lock = PTHREAD_MUTEX_INITIALIZER;
static uint64_t marks[MAX_MARKS];
static size_t n_marks;
static struct discon discons[DISCON_SLOTS];
static FILE *snaps;
static unsigned sequence;
static char *prefix;
static int count_discon = 1;

static qemu_plugin_u64 counter(size_t index)
{
    return (qemu_plugin_u64){score, index * sizeof(uint64_t)};
}

static uint64_t mix(uint64_t hash, uint64_t value)
{
    for (int i = 0; i < 8; ++i) {
        hash ^= (value >> (8 * i)) & 0xff;
        hash *= 1099511628211ull;
    }
    return hash;
}

/* Return the block index, or SIZE_MAX when the table is full. */
static size_t find_or_add(uint64_t pc, uint64_t key, const uint32_t *words, size_t n)
{
    size_t slot = key & mask;
    for (;;) {
        uint64_t entry = table[slot];
        if (!entry)
            break;
        struct block *b = &blocks[entry - 1];
        if (b->key == key && b->pc == pc && b->n == n
            && !memcmp(pool + b->code, words, n * sizeof(*words)))
            return entry - 1;
        slot = (slot + 1) & mask;
    }
    if (used == capacity)
        return SIZE_MAX;
    if (pool_used + n > pool_cap) {
        size_t grow = pool_cap * 2 + n;
        uint32_t *next = realloc(pool, grow * sizeof(*pool));
        if (!next)
            return SIZE_MAX;
        pool = next;
        pool_cap = grow;
    }
    memcpy(pool + pool_used, words, n * sizeof(*words));
    blocks[used] = (struct block){pc, key, (uint32_t)n, (uint32_t)pool_used};
    pool_used += n;
    table[slot] = used + 1;
    return used++;
}

static void snapshot(const char *label, unsigned vcpu)
{
    struct timespec now;
    pthread_mutex_lock(&snap_lock);
    pthread_mutex_lock(&lock);
    size_t count = used;
    pthread_mutex_unlock(&lock);
    clock_gettime(CLOCK_MONOTONIC, &now);
    fprintf(snaps, "S %u %s %u %lld.%09ld %zu\n", sequence++, label, vcpu,
            (long long)now.tv_sec, now.tv_nsec, count);
    for (size_t i = 0; i < count; ++i) {
        uint64_t value = qemu_plugin_u64_sum(counter(i));
        if (value != last[i]) {
            fprintf(snaps, "%zu %" PRIu64 "\n", i, value - last[i]);
            last[i] = value;
        }
    }
    fputs("E\n", snaps);
    fflush(snaps);
    pthread_mutex_unlock(&snap_lock);
}

static void on_mark(unsigned int vcpu, void *userdata)
{
    char label[32];
    snprintf(label, sizeof(label), "mark%u", (unsigned)(uintptr_t)userdata);
    snapshot(label, vcpu);
}

static void on_discon(unsigned int vcpu, enum qemu_plugin_discon_type type,
                      uint64_t from_pc, uint64_t to_pc, void *userdata)
{
    (void)vcpu;
    (void)userdata;
    uint32_t high = (uint32_t)(from_pc >> 63);
    size_t slot = mix(mix(1469598103934665603ull, to_pc), type * 2 + high) & (DISCON_SLOTS - 1);
    pthread_mutex_lock(&discon_lock);
    size_t probe = 0;
    for (; probe < DISCON_SLOTS; ++probe, slot = (slot + 1) & (DISCON_SLOTS - 1)) {
        struct discon *d = &discons[slot];
        if (!d->count) {
            *d = (struct discon){to_pc, 1, (uint32_t)type, high};
            break;
        }
        if (d->to_pc == to_pc && d->type == (uint32_t)type && d->from_high == high) {
            d->count++;
            break;
        }
    }
    if (probe == DISCON_SLOTS)
        discon_dropped++;
    pthread_mutex_unlock(&discon_lock);
}

static void translate(struct qemu_plugin_tb *tb, void *userdata)
{
    (void)userdata;
    uint32_t words[MAX_INSNS];
    size_t n = qemu_plugin_tb_n_insns(tb);
    uint64_t pc = qemu_plugin_tb_vaddr(tb);
    if (!n || n > MAX_INSNS) {
        __atomic_fetch_add(&irregular, 1, __ATOMIC_RELAXED);
        return;
    }
    uint64_t key = mix(mix(1469598103934665603ull, pc), n);
    for (size_t i = 0; i < n; ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        if (qemu_plugin_insn_size(insn) != 4 || qemu_plugin_insn_vaddr(insn) != pc + 4 * i
            || qemu_plugin_insn_data(insn, &words[i], 4) != 4) {
            __atomic_fetch_add(&irregular, 1, __ATOMIC_RELAXED);
            return;
        }
        key = mix(key, words[i]);
    }
    key |= 1;
    pthread_mutex_lock(&lock);
    size_t index = find_or_add(pc, key, words, n);
    if (index == SIZE_MAX)
        dropped++;
    pthread_mutex_unlock(&lock);
    if (index != SIZE_MAX)
        qemu_plugin_register_vcpu_tb_exec_inline_per_vcpu(tb, QEMU_PLUGIN_INLINE_ADD_U64,
                                                          counter(index), 1);
    for (size_t m = 0; m < n_marks; ++m)
        if (marks[m] == pc)
            qemu_plugin_register_vcpu_tb_exec_cb(tb, on_mark, QEMU_PLUGIN_CB_NO_REGS,
                                                 (void *)(uintptr_t)m);
}

static FILE *open_output(const char *suffix)
{
    char path[4096];
    snprintf(path, sizeof(path), "%s.%s", prefix, suffix);
    return fopen(path, "w");
}

static void finish(void *userdata)
{
    (void)userdata;
    snapshot("exit", 0);
    FILE *out = open_output("blocks");
    if (out) {
        fprintf(out, "# blocks %zu capacity %zu dropped %" PRIu64 " irregular %" PRIu64 "\n",
                used, capacity, dropped, irregular);
        for (size_t i = 0; i < used; ++i) {
            fprintf(out, "%zu %" PRIx64 " %u", i, blocks[i].pc, blocks[i].n);
            for (uint32_t k = 0; k < blocks[i].n; ++k)
                fprintf(out, " %08" PRIx32, pool[blocks[i].code + k]);
            fputc('\n', out);
        }
        fclose(out);
    }
    out = open_output("discon");
    if (out) {
        fprintf(out, "# type to_pc from_kernel count; dropped %" PRIu64 "\n", discon_dropped);
        for (size_t i = 0; i < DISCON_SLOTS; ++i)
            if (discons[i].count)
                fprintf(out, "%u %" PRIx64 " %u %" PRIu64 "\n", discons[i].type,
                        discons[i].to_pc, discons[i].from_high, discons[i].count);
        fclose(out);
    }
    fclose(snaps);
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info,
                                           int argc, char **argv)
{
    capacity = (size_t)1 << 21;
    if (!info->system_emulation || strcmp(info->target_name, "aarch64"))
        return 1;
    for (int i = 0; i < argc; ++i) {
        char *end = NULL;
        if (!strncmp(argv[i], "out=", 4) && argv[i][4]) {
            prefix = argv[i] + 4;
        } else if (!strncmp(argv[i], "mark=", 5) && n_marks < MAX_MARKS) {
            marks[n_marks++] = strtoull(argv[i] + 5, &end, 16);
            if (!end || *end)
                return 1;
        } else if (!strcmp(argv[i], "discon=off")) {
            count_discon = 0;
        } else if (!strncmp(argv[i], "capacity=", 9)) {
            capacity = strtoull(argv[i] + 9, &end, 10);
            if (!end || *end || capacity < 1024 || capacity > ((size_t)1 << 26))
                return 1;
        } else {
            return 1;
        }
    }
    if (!prefix)
        return 1;
    size_t slots = 1;
    while (slots < capacity * 2)
        slots <<= 1;
    mask = slots - 1;
    pool_cap = capacity * 4;
    blocks = calloc(capacity, sizeof(*blocks));
    table = calloc(slots, sizeof(*table));
    last = calloc(capacity, sizeof(*last));
    pool = malloc(pool_cap * sizeof(*pool));
    snaps = open_output("snaps");
    if (!blocks || !table || !last || !pool || !snaps)
        return 1;
    score = qemu_plugin_scoreboard_new(capacity * sizeof(uint64_t));
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    if (count_discon)
        qemu_plugin_register_vcpu_discon_cb(id, QEMU_PLUGIN_DISCON_ALL, on_discon, NULL);
    qemu_plugin_register_atexit_cb(id, finish, NULL);
    return 0;
}
