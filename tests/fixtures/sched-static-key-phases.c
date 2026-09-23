// SPDX-License-Identifier: GPL-2.0-only
/* K01 observer: distinguish the publication phase of each missing-pointer
 * observation. No object dereference/free, hotplug, BPF or production change. */
#include <linux/delay.h>
#include <linux/jump_label.h>
#include <linux/kthread.h>
#include <linux/module.h>
#include <linux/rcupdate.h>
#include <linux/sched.h>

static DEFINE_STATIC_KEY_FALSE(published_key);
static int object;
static int __rcu *published;
static bool memory_gate, reader_preempt;
module_param(memory_gate, bool, 0400);
module_param(reader_preempt, bool, 0400);
static bool memory_enabled;
static u32 phase;
static u32 *gate_word;

struct phase_reader {
    struct task_struct *task;
    unsigned long reads, missing, stable[8], changing;
    u32 first_before, first_after, first_instruction;
    int first_key, first_nesting;
};
static struct phase_reader readers[4];

static int read_published(void *arg)
{
    struct phase_reader *r = arg;

    while (!kthread_should_stop()) {
        for (unsigned int i = 0; i < 256; ++i) {
            u32 before, after;

            if (reader_preempt)
                preempt_disable();
            rcu_read_lock();
            before = READ_ONCE(phase);
            if (memory_gate ? smp_load_acquire(&memory_enabled)
                            : static_branch_unlikely(&published_key)) {
                int *p = rcu_dereference(published);
                after = READ_ONCE(phase);
                ++r->reads;
                if (!p) {
                    if (!r->missing) {
                        r->first_before = before;
                        r->first_after = after;
                        r->first_instruction = READ_ONCE(*gate_word);
                        r->first_key = atomic_read(&published_key.key.enabled);
                        r->first_nesting = current->rcu_read_lock_nesting;
                    }
                    ++r->missing;
                    if (before == after)
                        ++r->stable[before & 7];
                    else
                        ++r->changing;
                }
            }
            rcu_read_unlock();
            if (reader_preempt)
                preempt_enable();
        }
        cond_resched();
    }
    return 0;
}

static int __init check_phases(void)
{
    unsigned int nr = 0, rounds = 0;
    unsigned long reads = 0, missing = 0;
    int cpu, ret = 0;

    for (unsigned int i = 0; i < THIS_MODULE->num_jump_entries; ++i) {
        struct jump_entry *e = &THIS_MODULE->jump_entries[i];
        if (jump_entry_key(e) == &published_key.key)
            gate_word = (u32 *)jump_entry_code(e);
    }
    if (!gate_word)
        return -ENOENT;
    for_each_online_cpu(cpu) {
        struct phase_reader *r = &readers[nr];
        r->task = kthread_create(read_published, r, "phase-reader/%u", nr);
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
        for (; rounds < 256; ++rounds) {
            WRITE_ONCE(phase, rounds * 8 + 1);
            rcu_assign_pointer(published, &object);
            WRITE_ONCE(phase, rounds * 8 + 2);
            if (memory_gate)
                smp_store_release(&memory_enabled, true);
            else
                static_branch_enable(&published_key);
            WRITE_ONCE(phase, rounds * 8 + 3);
            usleep_range(100, 200);
            WRITE_ONCE(phase, rounds * 8 + 4);
            if (memory_gate)
                smp_store_release(&memory_enabled, false);
            else
                static_branch_disable(&published_key);
            WRITE_ONCE(phase, rounds * 8 + 5);
            synchronize_rcu();
            WRITE_ONCE(phase, rounds * 8 + 6);
            RCU_INIT_POINTER(published, NULL);
            WRITE_ONCE(phase, rounds * 8 + 7);
            usleep_range(100, 200);
        }
    }
    for (unsigned int i = 0; i < nr; ++i) {
        struct phase_reader *r = &readers[i];
        kthread_stop(r->task);
        reads += r->reads;
        missing += r->missing;
        pr_info("KEY_PHASE reader=%u reads=%lu missing=%lu changing=%lu stable=%lu,%lu,%lu,%lu,%lu,%lu,%lu,%lu first=%u/%u instruction=%08x key=%d nesting=%d\n",
                i, r->reads, r->missing, r->changing,
                r->stable[0], r->stable[1], r->stable[2], r->stable[3],
                r->stable[4], r->stable[5], r->stable[6], r->stable[7],
                r->first_before, r->first_after, r->first_instruction,
                r->first_key, r->first_nesting);
    }
    pr_info("KEY_PHASE_TOTAL memory_gate=%d preempt_guard=%d readers=%u rounds=%u reads=%lu missing=%lu\n",
            memory_gate, reader_preempt, nr, rounds, reads, missing);
    if (ret)
        return ret;
    return nr < 2 || !reads || missing ? -EUCLEAN : 0;
}

static void __exit stop_phases(void) {}
module_init(check_phases);
module_exit(stop_phases);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Static-key RCU publication phase observer");
