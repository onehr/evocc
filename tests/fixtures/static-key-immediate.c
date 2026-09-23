// SPDX-License-Identifier: GPL-2.0-only
/* K01 controls without RCU, object publication, hotplug or BPF: immediate
 * local/remote visibility, then concurrent queries during stable intervals.
 * An odd sequence excludes updates; matching even sequences bound each query.
 * These are diagnostics, never a proposed change to production kernel code. */
#include <linux/delay.h>
#include <linux/jump_label.h>
#include <linux/kthread.h>
#include <linux/module.h>
#include <linux/smp.h>

static DEFINE_STATIC_KEY_FALSE(probe_key);
static atomic_t remote_misses = ATOMIC_INIT(0);
static unsigned int sequence;
static bool memory_gate;
module_param(memory_gate, bool, 0400);

struct key_reader {
    struct task_struct *task;
    unsigned long checked, missing;
    unsigned int first_sequence;
    bool first_value;
};
static struct key_reader readers[4];

static noinline bool query_key(void)
{
    return memory_gate ? static_key_enabled(&probe_key.key)
                       : static_branch_unlikely(&probe_key);
}

static void check_cpu(void *expected)
{
    if (query_key() != !!expected)
        atomic_inc(&remote_misses);
}

static int check_stable(void *arg)
{
    struct key_reader *r = arg;

    while (!kthread_should_stop()) {
        unsigned int before = smp_load_acquire(&sequence);
        bool value;

        if (before & 1)
            continue;
        value = query_key();
        /* A control dependency alone would not order the second observation. */
        smp_mb();
        if (READ_ONCE(sequence) != before)
            continue;
        ++r->checked;
        if (value != !!(before & 2)) {
            if (!r->missing) {
                r->first_sequence = before;
                r->first_value = value;
            }
            ++r->missing;
        }
        cond_resched();
    }
    return 0;
}

static int __init check_immediate(void)
{
    unsigned int enable_misses = 0, disable_misses = 0;
    unsigned int nr = 0;
    unsigned long checked = 0, missing = 0;
    int cpu, ret = 0;

    if (query_key())
        return -EUCLEAN;
    for (unsigned int i = 0; i < 256; ++i) {
        static_branch_enable(&probe_key);
        if (!query_key())
            ++enable_misses;
        on_each_cpu(check_cpu, (void *)1UL, 1);
        static_branch_disable(&probe_key);
        if (query_key())
            ++disable_misses;
        on_each_cpu(check_cpu, NULL, 1);
    }
    pr_info("STATIC_KEY_IMMEDIATE rounds=256 enable_misses=%u disable_misses=%u remote_misses=%d\n",
            enable_misses, disable_misses, atomic_read(&remote_misses));
    for_each_online_cpu(cpu) {
        struct key_reader *r = &readers[nr];
        r->task = kthread_create(check_stable, r, "stable-key/%u", nr);
        if (IS_ERR(r->task)) {
            ret = PTR_ERR(r->task);
            break;
        }
        kthread_bind(r->task, cpu);
        wake_up_process(r->task);
        if (++nr == ARRAY_SIZE(readers))
            break;
    }
    if (!ret) {
        for (unsigned int i = 0; i < 512; ++i) {
            WRITE_ONCE(sequence, i * 2 + 1);
            smp_mb();
            if (i & 1)
                static_branch_disable(&probe_key);
            else
                static_branch_enable(&probe_key);
            smp_store_release(&sequence, i * 2 + 2);
            usleep_range(100, 200);
        }
    }
    for (unsigned int i = 0; i < nr; ++i) {
        kthread_stop(readers[i].task);
        checked += readers[i].checked;
        missing += readers[i].missing;
        if (readers[i].missing)
            pr_info("STATIC_KEY_STABLE_MISSING reader=%u sequence=%u value=%d\n",
                    i, readers[i].first_sequence, readers[i].first_value);
    }
    pr_info("STATIC_KEY_STABLE memory_gate=%d readers=%u checked=%lu missing=%lu\n",
            memory_gate, nr, checked, missing);
    if (ret)
        return ret;
    return enable_misses || disable_misses || atomic_read(&remote_misses)
        || nr < 2 || !checked || missing ? -EUCLEAN : 0;
}

static void __exit stop_immediate(void) {}
module_init(check_immediate);
module_exit(stop_immediate);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Immediate and concurrent static-key consistency controls");
