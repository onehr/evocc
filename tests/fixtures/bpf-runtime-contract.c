/* K10 reductions of bpf_test_rqspinlock's prerequisite and htab's reentry
 * error contract. Reference-built
 * diagnostic, not an additional target-C coverage credit. No lock stress,
 * worker or NMI execution occurs: counters remain disabled. */
#include <linux/module.h>
#include <linux/perf_event.h>
#include <asm/rqspinlock.h>

static void overflow(struct perf_event *event, struct perf_sample_data *data,
                     struct pt_regs *regs) {}

static int __init probe_init(void)
{
    struct perf_event_attr attr = {
        .type = PERF_TYPE_HARDWARE,
        .config = PERF_COUNT_HW_CPU_CYCLES,
        .size = sizeof(attr),
        .pinned = 1,
        .disabled = 1,
        .sample_period = 100000,
    };
    struct perf_event *event;
    long hardware, software;
    rqspinlock_t lock;
    unsigned long first_flags, second_flags;
    int first, reentrant = 0;

    event = perf_event_create_kernel_counter(&attr, 1, NULL, overflow, NULL);
    hardware = IS_ERR(event) ? PTR_ERR(event) : 0;
    if (!IS_ERR(event))
        perf_event_release_kernel(event);
    attr.type = PERF_TYPE_SOFTWARE;
    attr.config = PERF_COUNT_SW_CPU_CLOCK;
    event = perf_event_create_kernel_counter(&attr, 1, NULL, overflow, NULL);
    software = IS_ERR(event) ? PTR_ERR(event) : 0;
    if (!IS_ERR(event))
        perf_event_release_kernel(event);
    raw_res_spin_lock_init(&lock);
    first = raw_res_spin_lock_irqsave(&lock, first_flags);
    if (!first) {
        reentrant = raw_res_spin_lock_irqsave(&lock, second_flags);
        if (!reentrant)
            raw_res_spin_unlock_irqrestore(&lock, second_flags);
        raw_res_spin_unlock_irqrestore(&lock, first_flags);
    }
    pr_info("BPF_RUNTIME hardware=%ld software=%ld first=%d reentrant=%d\n",
            hardware, software, first, reentrant);
    /* The locked HVF lane has no PMU. Distinguish a missing event provider
     * from a missing .ko, signature error, or broken perf API as a whole. */
    return hardware == -ENOENT && software == 0 && !first &&
           reentrant == -EDEADLK ? 0 : -EINVAL;
}

static void __exit probe_exit(void) {}
module_init(probe_init);
module_exit(probe_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("BPF selftest prerequisite and reentrant-lock reduction");
