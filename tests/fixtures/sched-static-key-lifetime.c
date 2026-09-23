// SPDX-License-Identifier: GPL-2.0-only
/* K01 diagnostic reduction, not a scheduler fix. Readers enter an RCU section
 * before testing a static key. The writer publishes the object before enable,
 * disables the key, waits for readers, then clears the object. No hotplug, BPF,
 * object freeing, scheduler callbacks, GDB stops or intentional invalid access. */
#include <linux/delay.h>
#include <linux/jump_label.h>
#include <linux/kthread.h>
#include <linux/module.h>
#include <linux/rcupdate.h>
#include <linux/sched.h>

static DEFINE_STATIC_KEY_FALSE(published_key);
static int object;
static int __rcu *published;
static bool memory_gate;
module_param(memory_gate, bool, 0400);
static bool memory_enabled;
static u32 *gate_word;

struct reader {
    struct task_struct *task;
    unsigned long reads, missing;
    u32 first_missing_instruction;
    int first_missing_key;
};
static struct reader readers[4];

static int read_published(void *arg)
{
    struct reader *r = arg;

    while (!kthread_should_stop()) {
        for (unsigned int i = 0; i < 256; ++i) {
            rcu_read_lock();
#ifdef KEY_READER_ISB
            /* Diagnostic only: distinguish instruction synchronization from
             * the RCU/object-publication contract. Never a production fix. */
            asm volatile("isb" ::: "memory");
#endif
#ifdef KEY_READER_MB
            smp_mb();
#endif
            if (memory_gate ? smp_load_acquire(&memory_enabled)
                            : static_branch_unlikely(&published_key)) {
                int *p = rcu_dereference(published);
                ++r->reads;
                if (!p) {
                    if (!r->missing) {
                        r->first_missing_instruction = READ_ONCE(*gate_word);
                        r->first_missing_key = atomic_read(&published_key.key.enabled);
                    }
                    ++r->missing;
                }
            }
            rcu_read_unlock();
        }
        cond_resched();
    }
    return 0;
}

static int __init check_publication(void)
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
        struct reader *r = &readers[nr];
        r->task = kthread_create(read_published, r, "key-reader/%u", nr);
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
        for (; rounds < 1024; ++rounds) {
            rcu_assign_pointer(published, &object);
            if (memory_gate)
                smp_store_release(&memory_enabled, true);
            else
                static_branch_enable(&published_key);
            usleep_range(100, 200);
            if (memory_gate)
                smp_store_release(&memory_enabled, false);
            else
                static_branch_disable(&published_key);
            synchronize_rcu();
            RCU_INIT_POINTER(published, NULL);
            usleep_range(100, 200);
        }
    }
    for (unsigned int i = 0; i < nr; ++i) {
        kthread_stop(readers[i].task);
        reads += readers[i].reads;
        missing += readers[i].missing;
        if (readers[i].missing)
            pr_info("STATIC_KEY_MISSING reader=%u instruction=%08x key=%d\n", i,
                    readers[i].first_missing_instruction, readers[i].first_missing_key);
    }
    pr_info("STATIC_KEY_LIFETIME memory_gate=%d readers=%u rounds=%u reads=%lu missing=%lu\n",
            memory_gate, nr, rounds, reads, missing);
    if (ret)
        return ret;
    return nr < 2 || !reads || missing ? -EUCLEAN : 0;
}

static void __exit stop_publication(void) {}
module_init(check_publication);
module_exit(stop_publication);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Concurrent static-key/RCU publication diagnostic");
